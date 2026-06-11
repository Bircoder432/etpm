#ifndef ETPM_H
#define ETPM_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Opaque handle to the ETPM Manager
typedef struct EtpmManager EtpmManager;

// Status codes
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
    ETPM_ERR_UNKNOWN = 99
} EtpmStatus;

// Creates a new ETPM manager. Returns NULL on failure.
EtpmManager* etpm_manager_new();

// Frees the ETPM manager.
void etpm_manager_free(EtpmManager* manager);

// Sets the root directory for overlay extraction.
EtpmStatus etpm_set_root(EtpmManager* manager, const char* path);

// Sets the directory for package metadata (addition & filelist).
EtpmStatus etpm_set_packages(EtpmManager* manager, const char* path);

// Adds a repository URL.
EtpmStatus etpm_add_repository(EtpmManager* manager, const char* url);

// Fetches a package.
// On success, *out_path will point to a newly allocated string with the file path.
// The caller MUST free this string using etpm_free_string().
EtpmStatus etpm_fetch_package(
    EtpmManager* manager,
    const char* name,
    const char* version,
    const char* dest,
    char** out_path
);

// Installs a package from a local .tp archive.
EtpmStatus etpm_install_package(
    EtpmManager* manager,
    const char* path,
    const char* name,
    const char* version
);

// Uninstalls a package.
EtpmStatus etpm_uninstall_package(
    EtpmManager* manager,
    const char* name,
    const char* version
);

// Retrieves the last error message as a string.
// Returns NULL if no error occurred.
// The caller MUST free the returned string using etpm_free_string().
char* etpm_get_last_error(EtpmManager* manager);

// Frees a string allocated by the ETPM library.
void etpm_free_string(char* str);

// Adds a trusted key for package signature verification.
void etpm_add_trusted_key(EtpmManager* manager, const char* key);

// Enables or disables the requirement for package signature verification (1 = true, 0 = false).
EtpmStatus etpm_set_allow_unsigned(EtpmManager* manager, int allow);

#ifdef __cplusplus
}
#endif

#endif // ETPM_H
