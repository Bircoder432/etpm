#include <stdio.h>
#include <stdlib.h>
#include "etpm.h"

void print_error(EtpmManager* manager, const char* context) {
    char* err = etpm_get_last_error(manager);
    if (err) {
        fprintf(stderr, "Error during %s: %s\n", context, err);
        etpm_free_string(err);
    } else {
        fprintf(stderr, "Unknown error during %s\n", context);
    }
}

int main() {
    printf("Initializing ETPM...\n");
    EtpmManager* manager = etpm_manager_new();
    if (!manager) {
        fprintf(stderr, "Failed to create ETPM manager\n");
        return 1;
    }

    // 1. Configure paths
    if (etpm_set_root(manager, "./testroot_c") != ETPM_OK) {
        print_error(manager, "set_root");
        return 1;
    }
    if (etpm_set_packages(manager, "./testroot_c/packages") != ETPM_OK) {
        print_error(manager, "set_packages");
        return 1;
    }

    // 2. Add repository
    if (etpm_add_repository(manager, "http://127.0.0.1:22869/") != ETPM_OK) {
        print_error(manager, "add_repository");
        return 1;
    }
    printf("Repository added successfully.\n");

    // 3. Fetch package
    char* downloaded_path = NULL;
    printf("Fetching package...\n");
    EtpmStatus status = etpm_fetch_package(
        manager,
        "example-package",
        "1.0.0",
        ".",
        &downloaded_path
    );

    if (status != ETPM_OK) {
        print_error(manager, "fetch_package");
        etpm_manager_free(manager);
        return 1;
    }
    printf("Package downloaded to: %s\n", downloaded_path);

    // 4. Install package
    printf("Installing package...\n");
    if (etpm_install_package(manager, downloaded_path, "example-package", "1.0.0") != ETPM_OK) {
        print_error(manager, "install_package");
        etpm_free_string(downloaded_path);
        etpm_manager_free(manager);
        return 1;
    }
    printf("Package installed successfully!\n");

    // Clean up the fetched file path string
    etpm_free_string(downloaded_path);

    // 5. Uninstall package
    printf("Uninstalling package...\n");
    if (etpm_uninstall_package(manager, "example-package", "1.0.0") != ETPM_OK) {
        print_error(manager, "uninstall_package");
        etpm_manager_free(manager);
        return 1;
    }
    printf("Package uninstalled successfully!\n");

    // 6. Cleanup
    etpm_manager_free(manager);
    printf("ETPM shutdown complete.\n");

    return 0;
}
