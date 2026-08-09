#ifndef WML2VIEWER_IOS_H
#define WML2VIEWER_IOS_H

#include <stdint.h>
#include <stddef.h>

int32_t wml2viewer_ios_main(const char *app_support_dir,
                            const char *documents_dir,
                            const char *caches_dir);

void wml2viewer_ios_initialize_bridge(void);
void wml2viewer_ios_receive_external_path(const char *path);

int32_t wml2viewer_ios_keychain_save_password(const char *reference,
                                              const char *password);
int32_t wml2viewer_ios_keychain_copy_password(const char *reference,
                                              char *output,
                                              uintptr_t capacity);

int32_t wml2viewer_ios_request_folder_picker(void);
int32_t wml2viewer_ios_request_file_picker(void);

#endif
