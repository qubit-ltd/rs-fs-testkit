//! Independent SDK-backed S3 contract fixture.

mod fixture;

pub use fixture::Fixture;

#[allow(dead_code, reason = "shared test module also compiled by listing-only binaries")]
pub mod recovery_matrix;
