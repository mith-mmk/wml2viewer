#ifndef WML2VIEWER_IOS_H
#define WML2VIEWER_IOS_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* A return value of 1 is success/true; 0 is failure/false. Handles are nonzero. */
uint64_t wml2viewer_ios_session_create(void);
uint8_t wml2viewer_ios_session_release(uint64_t session);
uint64_t wml2viewer_ios_request_next(uint64_t session);
uint8_t wml2viewer_ios_request_begin(uint64_t session, uint64_t request);
uint8_t wml2viewer_ios_request_cancel(uint64_t session, uint64_t request);
uint8_t wml2viewer_ios_request_is_current(uint64_t session, uint64_t request);

uint64_t wml2viewer_ios_decode_local(uint64_t session, uint64_t request,
                                     const uint8_t *path, size_t path_length,
                                     const uint8_t *mime, size_t mime_length);
uint8_t wml2viewer_ios_image_release(uint64_t image);
int32_t wml2viewer_ios_image_width(uint64_t image);
int32_t wml2viewer_ios_image_height(uint64_t image);
int32_t wml2viewer_ios_image_stride(uint64_t image);
uint8_t wml2viewer_ios_image_rgba(uint64_t image, const uint8_t **data,
                                  size_t *length);
size_t wml2viewer_ios_image_frame_count(uint64_t image);
int64_t wml2viewer_ios_image_loop_count(uint64_t image);
uint8_t wml2viewer_ios_image_frame_duration_ms(uint64_t image, size_t index,
                                               uint64_t *duration_ms);
uint64_t wml2viewer_ios_image_frame(uint64_t image, size_t index);

uint64_t wml2viewer_ios_archive_open_local(uint64_t session, uint64_t request,
                                           const uint8_t *path, size_t path_length,
                                           const uint8_t *format, size_t format_length);
uint8_t wml2viewer_ios_archive_release(uint64_t archive);
size_t wml2viewer_ios_archive_entry_count(uint64_t archive);
uint8_t wml2viewer_ios_archive_entry_name(uint64_t archive, size_t index,
                                          uint8_t *output, size_t capacity,
                                          size_t *output_length);
uint8_t wml2viewer_ios_archive_entry_size(uint64_t archive, size_t index,
                                          uint64_t *size, uint8_t *known);
uint64_t wml2viewer_ios_archive_entry_decode(uint64_t session, uint64_t request,
                                             uint64_t archive, size_t index,
                                             const uint8_t *mime, size_t mime_length);
uint64_t wml2viewer_ios_archive_entry_materialize(uint64_t session, uint64_t request,
                                                  uint64_t archive, size_t index);

uint8_t wml2viewer_ios_bytes_release(uint64_t bytes);
uint8_t wml2viewer_ios_bytes_view(uint64_t bytes, const uint8_t **data, size_t *length);
uint64_t wml2viewer_ios_encode_rgba(uint64_t session, uint64_t request,
                                    const uint8_t *rgba, size_t rgba_length,
                                    int32_t width, int32_t height, int32_t stride,
                                    const uint8_t *format, size_t format_length);

int32_t wml2viewer_ios_request_error_code(uint64_t session, uint64_t request);
uint8_t wml2viewer_ios_request_error_key(uint64_t session, uint64_t request,
                                         uint8_t *output, size_t capacity,
                                         size_t *output_length);
uint8_t wml2viewer_ios_request_error_args_json(uint64_t session, uint64_t request,
                                               uint8_t *output, size_t capacity,
                                               size_t *output_length);

uint8_t wml2viewer_ios_plan_reading_v1(const int64_t *source_ids,
                                       const uint8_t *portrait,
                                       const uint8_t *covers,
                                       size_t page_count,
                                       int32_t current_index,
                                       uint8_t landscape,
                                       int32_t layout,
                                       int32_t direction,
                                       uint8_t cover_alone,
                                       int32_t maximum_prefetch_spreads,
                                       int32_t *output,
                                       size_t capacity,
                                       size_t *output_length);

/*
 * Variable-length UTF-8 and reading-wire functions support a size query by
 * passing output == NULL and capacity == 0. They write the required element
 * count to output_length. Image/byte views are borrowed and must be copied
 * before releasing their owning handle.
 */

#ifdef __cplusplus
}
#endif

#endif
