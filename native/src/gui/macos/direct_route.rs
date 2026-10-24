//! Internal typed direct-recipient condition for the reviewed Mach-O build.
//! Caller owns original scope, deadline and final consumed-field witnesses.
//! No tool/read registration, input permission or atomic/cache-completion proof.
mod envelope {
    pub(super) use super::super::hit_envelope as basis;
    pub(super) use super::super::hit_envelope::variants::{
        frame, masked, progress, Common, Decision, FrameRenderer, FrameResource, GeometryError,
        MaskedRenderer, MaskedResource, ProgressRenderer, ProgressResource, ProgressTrig,
        Unsupported,
    };
}
mod model;
mod typed;
mod witness;

// Provisional internal APIs have no public caller or input registration yet.
#[cfg(target_os = "macos")]
#[allow(unused_imports)]
pub(crate) use typed::{observe, TypedObservation};
#[cfg(target_os = "macos")]
#[allow(unused_imports)]
pub(crate) use witness::Witness;
