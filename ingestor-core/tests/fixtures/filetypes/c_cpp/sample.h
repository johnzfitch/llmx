/**
 * Sample C header file for testing.
 */

#ifndef SAMPLE_H
#define SAMPLE_H

#ifdef __cplusplus
extern "C" {
#endif

#define SAMPLE_VERSION "1.0.0"
#define MAX_BUFFER_SIZE 4096

/* Error codes */
typedef enum {
    SAMPLE_OK = 0,
    SAMPLE_ERROR_INVALID_ARG = -1,
    SAMPLE_ERROR_NOT_FOUND = -2,
    SAMPLE_ERROR_FULL = -3,
} SampleError;

/* Forward declarations */
struct Sample;
typedef struct Sample Sample;

/* Function declarations */

/**
 * Create a new sample instance.
 * @param name Instance name
 * @return New sample instance, or NULL on error
 */
Sample* sample_create(const char* name);

/**
 * Destroy a sample instance.
 * @param sample Instance to destroy
 */
void sample_destroy(Sample* sample);

/**
 * Get the sample's name.
 * @param sample The sample instance
 * @return The name string
 */
const char* sample_get_name(const Sample* sample);

/**
 * Process data through the sample.
 * @param sample The sample instance
 * @param data Input data buffer
 * @param len Data length
 * @return Number of bytes processed, or negative error code
 */
int sample_process(Sample* sample, const void* data, size_t len);

#ifdef __cplusplus
}
#endif

#endif /* SAMPLE_H */
