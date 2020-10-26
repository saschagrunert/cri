//! Port Manager related FFI interfaces

use crate::{
    ffi::error::{remove_last_error, update_last_err_if_required, update_last_error},
    network::cni::port::{PortManager, PortMapping as NativePortMappings, PortMappingBuilder},
};
use anyhow::{anyhow, bail, format_err, Context, Result};
use async_trait::async_trait;
use dyn_clone::{clone_trait_object, DynClone};
use ipnetwork::IpNetwork;
use libc::{c_char, c_void};
use std::{ffi::CStr, net::SocketAddr, ptr, slice};
use tokio::runtime::Runtime;

#[async_trait]
trait Manager: DynClone + Send + Sync {
    async fn add_impl(
        &mut self,
        _id: &str,
        _container_network: IpNetwork,
        _port_mappings: &[NativePortMappings],
    ) -> Result<()> {
        Ok(())
    }

    async fn remove_impl(&mut self, _id: &str) -> Result<()> {
        Ok(())
    }
}

clone_trait_object!(Manager);

#[async_trait]
impl Manager for PortManager {
    async fn add_impl(
        &mut self,
        id: &str,
        container_network: IpNetwork,
        port_mappings: &[NativePortMappings],
    ) -> Result<()> {
        self.add(id, container_network, port_mappings).await
    }

    async fn remove_impl(&mut self, id: &str) -> Result<()> {
        self.remove(id).await
    }
}

#[derive(Debug)]
#[repr(C)]
/// Port mappings added to the port manager.
pub struct PortMappings {
    /// The array of data containing the port mappings.
    array: *const PortMapping,

    /// Length of the `array`.
    length: usize,
}

#[derive(Debug)]
#[repr(C)]
/// A port mapping.
pub struct PortMapping {
    /// Host socket address to be used.
    host_ip: *const c_char,

    /// The port number on the host.
    host_port: u16,

    /// The port number inside the container.
    container_port: u16,

    /// The protocol of the port mapping.
    protocol: *const c_char,
}

#[no_mangle]
/// Create a new port manager instance. In case of any error, it will return a
/// `NULL` pointer and set the globally available last error.
pub extern "C" fn port_manager_new(storage_path: *const c_char) -> *mut c_void {
    match port_manager_new_res(storage_path) {
        Err(e) => {
            update_last_error(e);
            ptr::null_mut()
        }
        Ok(port_manager) => {
            remove_last_error();
            port_manager
        }
    }
}

fn port_manager_new_res(storage_path: *const c_char) -> Result<*mut c_void> {
    if storage_path.is_null() {
        bail!("provided storage path is NULL")
    }
    Ok(Box::into_raw(Box::new(Box::new(
        Runtime::new()?
            .block_on(PortManager::new(
                unsafe { CStr::from_ptr(storage_path) }
                    .to_str()
                    .context("convert storage path string")?,
            ))
            .context("create port manager")?,
    ) as Box<dyn Manager>)) as *mut c_void)
}

#[no_mangle]
/// Destroy the port manager instance and cleanup its used resources.
/// Populates the last error on failure.
pub extern "C" fn port_manager_destroy(port_manager: *mut c_void) {
    if port_manager.is_null() {
        update_last_error(anyhow!("provided port manager is NULL"));
    } else {
        unsafe { Box::from_raw(port_manager as *mut Box<dyn Manager>) };
        remove_last_error();
    }
}

#[no_mangle]
/// Add port mappings to the port manager.
/// Populates the last error on failure.
pub extern "C" fn port_manager_add(
    port_manager: *mut c_void,
    id: *const c_char,
    container_network: *const c_char,
    port_mappings: *const PortMappings,
) {
    update_last_err_if_required(port_manager_add_res(
        port_manager,
        id,
        container_network,
        port_mappings,
    ));
}

fn port_manager_add_res(
    port_manager: *mut c_void,
    id: *const c_char,
    container_network: *const c_char,
    port_mappings: *const PortMappings,
) -> Result<()> {
    if port_manager.is_null() {
        bail!("provided port manager is NULL")
    }
    if id.is_null() {
        bail!("provided ID is NULL")
    }
    if container_network.is_null() {
        bail!("provided container network is NULL")
    }
    if port_mappings.is_null() {
        bail!("provided port mappings are NULL")
    }

    let port_mappings_slice: &[PortMapping] =
        unsafe { slice::from_raw_parts((*port_mappings).array, (*port_mappings).length) };
    let mut mappings = Vec::with_capacity(port_mappings_slice.len());

    for mapping in port_mappings_slice {
        if mapping.host_ip.is_null() {
            bail!("port mapping host IP is NULL")
        }
        if mapping.protocol.is_null() {
            bail!("port mapping protocol is NULL")
        }

        let socket_addr_str = format!(
            "{}:{}",
            unsafe { CStr::from_ptr(mapping.host_ip) }
                .to_str()
                .context("convert host IP string")?,
            mapping.host_port,
        );
        mappings.push(
            PortMappingBuilder::default()
                .host(
                    socket_addr_str
                        .parse::<SocketAddr>()
                        .with_context(|| format_err!("parse socket address {}", socket_addr_str))?,
                )
                .container_port(mapping.container_port)
                .protocol(
                    unsafe { CStr::from_ptr(mapping.protocol) }
                        .to_str()
                        .context("convert protocol string")?,
                )
                .build()
                .context("build port mapping")?,
        );
    }

    let container_network_str = unsafe { CStr::from_ptr(container_network) }
        .to_str()
        .context("convert container network string")?;

    Runtime::new()?
        .block_on(
            unsafe {
                (port_manager as *mut Box<dyn Manager>)
                    .as_mut()
                    .context("retrieve port manager")?
            }
            .add_impl(
                unsafe { CStr::from_ptr(id) }
                    .to_str()
                    .context("convert ID string")?,
                container_network_str.parse().with_context(|| {
                    format_err!("parse container network {}", container_network_str)
                })?,
                &mappings,
            ),
        )
        .context("add port mappings")
}

#[no_mangle]
/// Remove all port mappings from the port manager for the provided `id`.
/// Populates the last error on failure.
pub extern "C" fn port_manager_remove(port_manager: *mut c_void, id: *const c_char) {
    update_last_err_if_required(port_manager_remove_res(port_manager, id))
}

fn port_manager_remove_res(port_manager: *mut c_void, id: *const c_char) -> Result<()> {
    if port_manager.is_null() {
        bail!("provided port manager is NULL")
    }
    if id.is_null() {
        bail!("provided ID is NULL")
    }

    Runtime::new()?
        .block_on(
            unsafe {
                (port_manager as *mut Box<dyn Manager>)
                    .as_mut()
                    .context("retrieve port manager")?
            }
            .remove_impl(
                unsafe { CStr::from_ptr(id) }
                    .to_str()
                    .context("convert ID string")?,
            ),
        )
        .context("remove port mappings")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::error::last_error_length;
    use std::ffi::CString;
    use tempfile::tempdir;

    #[test]
    fn new_port_manager_success() -> Result<()> {
        let temp_dir = tempdir()?;
        let c_string = CString::new(temp_dir.path().display().to_string())?;
        let port_manager = port_manager_new(c_string.into_raw());
        assert_eq!(last_error_length(), 0);
        port_manager_destroy(port_manager);
        Ok(())
    }

    #[test]
    fn new_port_manager_failure_wrong_storage_path() {
        let port_manager = port_manager_new("/some/wrong/path\0".as_ptr() as *const c_char);
        assert!(port_manager.is_null());
        assert!(last_error_length() > 0);
    }

    #[test]
    fn new_port_manager_failure_null() {
        let port_manager = port_manager_new(ptr::null());
        assert!(port_manager.is_null());
        assert!(last_error_length() > 0);
    }

    #[test]
    fn destroy_port_manager_failure() {
        port_manager_destroy(ptr::null_mut());
        assert!(last_error_length() > 0);
    }

    #[derive(Clone)]
    struct PortManagerMock;

    impl PortManagerMock {
        pub fn new() -> *mut c_void {
            Box::into_raw(Box::new(Box::new(PortManagerMock) as Box<dyn Manager>)) as *mut c_void
        }
    }

    #[async_trait]
    impl Manager for PortManagerMock {}

    #[test]
    fn add_port_mappings_success() {
        let port_manager = PortManagerMock::new();

        let mappings = PortMappings {
            array: [
                PortMapping {
                    host_ip: "127.0.0.1\0".as_ptr() as *const c_char,
                    host_port: 8080,
                    container_port: 8080,
                    protocol: "tcp\0".as_ptr() as *const c_char,
                },
                PortMapping {
                    host_ip: "127.0.0.1\0".as_ptr() as *const c_char,
                    host_port: 8081,
                    container_port: 8081,
                    protocol: "tcp\0".as_ptr() as *const c_char,
                },
            ]
            .as_ptr(),
            length: 2,
        };

        port_manager_add(
            port_manager,
            "id\0".as_ptr() as *const c_char,
            "127.0.0.1/8\0".as_ptr() as *const c_char,
            &mappings as *const PortMappings,
        );
        assert_eq!(last_error_length(), 0);

        port_manager_destroy(port_manager);
        assert_eq!(last_error_length(), 0);
    }

    #[test]
    fn add_port_mappings_failure_port_manager_null() {
        let mappings = PortMappings {
            array: [].as_ptr(),
            length: 0,
        };

        port_manager_add(
            ptr::null_mut() as *mut c_void,
            "id\0".as_ptr() as *const c_char,
            "127.0.0.1/8\0".as_ptr() as *const c_char,
            &mappings as *const PortMappings,
        );
        assert!(last_error_length() > 0);
    }

    #[test]
    fn add_port_mappings_failure_id_null() {
        let port_manager = PortManagerMock::new();

        let mappings = PortMappings {
            array: [].as_ptr(),
            length: 0,
        };

        port_manager_add(
            port_manager,
            ptr::null() as *const c_char,
            "127.0.0.1/8\0".as_ptr() as *const c_char,
            &mappings as *const PortMappings,
        );
        assert!(last_error_length() > 0);

        port_manager_destroy(port_manager);
        assert_eq!(last_error_length(), 0);
    }

    #[test]
    fn add_port_mappings_failure_container_network_null() {
        let port_manager = PortManagerMock::new();

        let mappings = PortMappings {
            array: [].as_ptr(),
            length: 0,
        };

        port_manager_add(
            port_manager,
            "id\0".as_ptr() as *const c_char,
            ptr::null() as *const c_char,
            &mappings as *const PortMappings,
        );
        assert!(last_error_length() > 0);

        port_manager_destroy(port_manager);
        assert_eq!(last_error_length(), 0);
    }

    #[test]
    fn add_port_mappings_failure_port_mappings_null() {
        let port_manager = PortManagerMock::new();

        port_manager_add(
            port_manager,
            "id\0".as_ptr() as *const c_char,
            "127.0.0.1/8\0".as_ptr() as *const c_char,
            ptr::null() as *const PortMappings,
        );
        assert!(last_error_length() > 0);

        port_manager_destroy(port_manager);
        assert_eq!(last_error_length(), 0);
    }

    #[test]
    fn add_port_mappings_failure_port_mapping_host_ip_null() {
        let port_manager = PortManagerMock::new();

        let mappings = PortMappings {
            array: [PortMapping {
                host_ip: ptr::null() as *const c_char,
                host_port: 8080,
                container_port: 8080,
                protocol: "tcp\0".as_ptr() as *const c_char,
            }]
            .as_ptr(),
            length: 1,
        };

        port_manager_add(
            port_manager,
            "id\0".as_ptr() as *const c_char,
            "127.0.0.1/8\0".as_ptr() as *const c_char,
            &mappings as *const PortMappings,
        );
        assert!(last_error_length() > 0);

        port_manager_destroy(port_manager);
        assert_eq!(last_error_length(), 0);
    }

    #[test]
    fn add_port_mappings_failure_port_mapping_protocol_null() {
        let port_manager = PortManagerMock::new();

        let mappings = PortMappings {
            array: [PortMapping {
                host_ip: "127.0.0.1\0".as_ptr() as *const c_char,
                host_port: 8080,
                container_port: 8080,
                protocol: ptr::null() as *const c_char,
            }]
            .as_ptr(),
            length: 1,
        };

        port_manager_add(
            port_manager,
            "id\0".as_ptr() as *const c_char,
            "127.0.0.1/8\0".as_ptr() as *const c_char,
            &mappings as *const PortMappings,
        );
        assert!(last_error_length() > 0);

        port_manager_destroy(port_manager);
        assert_eq!(last_error_length(), 0);
    }

    #[test]
    fn remove_port_mappings_success() {
        let port_manager = PortManagerMock::new();

        port_manager_remove(port_manager, "id\0".as_ptr() as *const c_char);
        assert_eq!(last_error_length(), 0);

        port_manager_destroy(port_manager);
        assert_eq!(last_error_length(), 0);
    }

    #[test]
    fn remove_port_mappings_failure_port_manager_null() {
        port_manager_remove(
            ptr::null_mut() as *mut c_void,
            "id\0".as_ptr() as *const c_char,
        );
        assert!(last_error_length() > 0);
    }

    #[test]
    fn remove_port_mappings_failure_id_null() {
        let port_manager = PortManagerMock::new();

        port_manager_remove(port_manager, ptr::null() as *const c_char);
        assert!(last_error_length() > 0);

        port_manager_destroy(port_manager);
        assert_eq!(last_error_length(), 0);
    }
}
