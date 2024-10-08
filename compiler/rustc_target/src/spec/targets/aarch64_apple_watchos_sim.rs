use crate::spec::base::apple::{opts, watchos_sim_llvm_target, Arch, TargetAbi};
use crate::spec::{FramePointer, Target, TargetOptions};

pub(crate) fn target() -> Target {
    let arch = Arch::Arm64;
    Target {
        // Clang automatically chooses a more specific target based on
        // WATCHOS_DEPLOYMENT_TARGET.
        // This is required for the simulator target to pick the right
        // MACH-O commands, so we do too.
        llvm_target: watchos_sim_llvm_target(arch).into(),
        metadata: crate::spec::TargetMetadata {
            description: Some("ARM64 Apple WatchOS Simulator".into()),
            tier: Some(3),
            host_tools: Some(false),
            std: Some(true),
        },
        pointer_width: 64,
        data_layout: "e-m:o-i64:64-i128:128-n32:64-S128-Fn32".into(),
        arch: arch.target_arch(),
        options: TargetOptions {
            features: "+neon,+fp-armv8,+apple-a7".into(),
            max_atomic_width: Some(128),
            frame_pointer: FramePointer::NonLeaf,
            ..opts("watchos", arch, TargetAbi::Simulator)
        },
    }
}
