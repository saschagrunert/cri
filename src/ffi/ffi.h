#pragma once

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

/**
 * A port mapping.
 */
typedef struct {
  /**
   * Host socket address to be used.
   */
  const char *host_ip;
  /**
   * The port number on the host.
   */
  uint16_t host_port;
  /**
   * The port number inside the container.
   */
  uint16_t container_port;
  /**
   * The protocol of the port mapping.
   */
  const char *protocol;
} PortMapping;

/**
 * Port mappings added to the port manager.
 */
typedef struct {
  /**
   * The array of data containing the port mappings.
   */
  const PortMapping *array;
  /**
   * Length of the `array`.
   */
  uintptr_t length;
} PortMappings;

/**
 * Calculate the number of bytes in the last error's error message including a
 * trailing `null` character. If there are no recent error, then this returns
 * `0`.
 */
int last_error_length(void);

/**
 * Write the most recent error message into a caller-provided buffer as a UTF-8
 * string, returning the number of bytes written.
 *
 * # Note
 *
 * This writes a **UTF-8** string into the buffer. Windows users may need to
 * convert it to a UTF-16 "unicode" afterwards.
 *
 * If there are no recent errors then this returns `0` (because we wrote 0
 * bytes). `-1` is returned if there are argument based errors, for example
 * when passed a `null` pointer or a buffer of insufficient size.
 */
int last_error_message(char *buffer, int length);

/**
 * Init the log level by the provided level string.
 * Populates the last error on any failure.
 */
void log_init(const char *level);

/**
 * Create a new port manager instance. In case of any error, it will return a
 * `NULL` pointer and set the globally available last error.
 */
void *port_manager_new(const char *storage_path);

/**
 * Destroy the port manager instance and cleanup its used resources.
 * Populates the last error on failure.
 */
void port_manager_destroy(void *port_manager);

/**
 * Add port mappings to the port manager.
 * Populates the last error on failure.
 */
void port_manager_add(void *port_manager,
                      const char *id,
                      const char *container_network,
                      const PortMappings *port_mappings);

/**
 * Remove all port mappings from the port manager for the provided `id`.
 * Populates the last error on failure.
 */
void port_manager_remove(void *port_manager, const char *id);
