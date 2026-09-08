//! Modernist shape tokens: there is only one radius, and it is zero.
//!
//! The Modernist system rounds nothing -- "Do not round a corner anywhere"
//! (see `crate::theme`'s module doc and the design guide). Every corner in
//! the Hub is square. The old per-component radius scale
//! (`XS`/`SM`/`MD`/`LG`/`FULL`) is kept as names so existing `styles::*`
//! call sites compile unchanged, but every one now resolves to `NONE`, so
//! flattening the whole UI needs no edit at any call site.

/// The single radius the system uses: none.
pub const NONE: f32 = 0.0;

/// Formerly text fields and pick lists. Now square.
pub const SM: f32 = NONE;
/// Formerly fully-rounded buttons and scrollbar thumbs. Now square, like
/// everything else -- Modernist has no pill.
pub const FULL: f32 = NONE;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_radius_is_zero() {
        for r in [NONE, SM, FULL] {
            assert_eq!(r, 0.0);
        }
    }
}
