#ifndef ETPM_H
#define ETPM_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct EtpmManager EtpmManager;

typedef enum {
    ETPM_OK = 0,
    ETPM_ERR_NULL_PTR = 1,
    ETPM_ERR_INVALID_UTF8 = 2,
    ETPM_ERR_PACKAGE_NOT_FOUND = 3,
    ETPM_ERR_IO = 4,
    ETPM_ERR_NETWORK = 5,
    ETPM_ERR_INVALID_VERSION = 6,
    ETPM_ERR_PATH_TRAVERSAL = 7,
    ETPM_ERR_REPOSITORY = 8,
    ETPM_ERR_RON_PARSE = 9,
    ETPM_ERR_URL_PARSE = 10,
    ETPM_ERR_INVALID_SIGNATURE = 11,
    ETPM_ERR_ADDITION_FILE_NOT_FOUND = 12,
    ETPM_ERR_INVALID_ADDITION_PATH = 13,
    ETPM_ERR_UNKNOWN = 99
} EtpmStatus;

EtpmManager* etpm_manager_new();
void etpm_manager_free(EtpmManager* manager);

EtpmStatus etpm_set_root(EtpmManager* manager, const char* path);
EtpmStatus etpm_set_packages(EtpmManager* manager, const char* path);
EtpmStatus etpm_add_repository(EtpmManager* manager, const char* url);
void etpm_add_trusted_key(EtpmManager* manager, const char* key);
EtpmStatus etpm_set_allow_unsigned(EtpmManager* manager, int allow);

EtpmStatus etpm_fetch_package(
    EtpmManager* manager,
    const char* name,
    const char* version,
    const char* dest,
    char** out_path
);

EtpmStatus etpm_install_package(
    EtpmManager* manager,
    const char* path,
    const char* name,
    const char* version
);

EtpmStatus etpm_uninstall_package(
    EtpmManager* manager,
    const char* name,
    const char* version
);

/// Reads a file from the addition directory of a downloaded package archive.
/// The caller must free the returned buffer using etpm_free_buffer.
EtpmStatus etpm_read_addition_file(
    EtpmManager* manager,
    const char* package_path,
    const char* file_path,
    uint8_t** out_data,
    size_t* out_len
);

char* etpm_get_last_error(EtpmManager* manager);
void etpm_free_string(char* str);

/// Frees a byte buffer allocated by the ETPM library.
void etpm_free_buffer(uint8_t* buffer, size_t len);

EtpmStatus etpm_fetch_addition_file(
    EtpmManager* manager,
    const char* package_name,
    const char* version,
    const char* addition_name,
    uint8_t** out_data,
    size_t* out_len
);

#ifdef __cplusplus
}
#endif

#endif // ETPM_H
