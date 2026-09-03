#ifndef WWN_VMS_ENGINE_H
#define WWN_VMS_ENGINE_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

enum {
  WWN_VM_ACCEL_HVF = 1,
  WWN_VM_ACCEL_TCTI = 2,
  WWN_VM_ACCEL_ANDROID_HV = 3,
  WWN_VM_ACCEL_TCG_JIT = 4,
  WWN_VM_ACCEL_TCG = 5
};

/** Compatibility API. iOS ignores mode_b_jit and uses its compiled flavor. */
int wwn_vm_preferred_accel(int mode_b_jit);

/** Preferred accel for this immutable engine product. */
int wwn_vm_product_accel(void);

/** 1 if /dev/kvm is present (Android / Linux HV path). */
int wwn_vm_android_kvm_available(void);

/** Static UTF-8 label for an accel enum value. */
const char *wwn_vm_accel_label(int accel);

/**
 * JSON LaunchSpec { accel, arch, memory_mb, qemu_bin, argv }.
 * Caller frees with wwn_vm_string_free. Returns NULL on error.
 */
char *wwn_vm_build_argv_json(const char *guest_dir, unsigned memory_mb,
                             int mode_b_jit, const char *qemu_dir,
                             const char *vsock_socket);

void wwn_vm_string_free(char *s);

#ifdef __cplusplus
}
#endif

#endif /* WWN_VMS_ENGINE_H */
