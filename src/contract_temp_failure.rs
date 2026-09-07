// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Original resource errors paired with their still-owned recovery session.

use std::error::Error;
use std::fmt::Debug;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;

/// A failed temporary lifecycle operation whose resource remains available for
/// recovery.
///
/// `T` is the synchronous or asynchronous temporary resource. Obtain this value
/// through [`crate::ContractSource::take`] and downcast to the concrete
/// resource type. Taking it transfers recovery responsibility to the caller.
/// Formatting does not inspect the resource or invoke the provider's error
/// formatter.
#[must_use]
pub struct ContractTempFailure<T> {
    error: Box<dyn Error + Send>,
    resource: T,
}

impl<T> ContractTempFailure<T> {
    /// Returns the original error for typed inspection.
    pub fn error(&self) -> &(dyn Error + Send + 'static) {
        self.error.as_ref()
    }

    /// Borrows the retained resource for lifecycle inspection.
    pub const fn resource(&self) -> &T {
        &self.resource
    }

    /// Borrows the retained resource for explicit recovery or cleanup.
    pub fn resource_mut(&mut self) -> &mut T {
        &mut self.resource
    }

    /// Transfers both the original error and its resource to the caller.
    pub fn into_parts(self) -> (Box<dyn Error + Send>, T) {
        (self.error, self.resource)
    }

    /// Retains a temporary resource failure before its resource can leave
    /// scope.
    pub(crate) fn new(error: impl Error + Send + 'static, resource: T) -> Self {
        Self {
            error: Box::new(error),
            resource,
        }
    }
}

impl<T> Debug for ContractTempFailure<T> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str("ContractTempFailure { resource: retained, error: retained }")
    }
}

impl<T> Display for ContractTempFailure<T> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str("temporary lifecycle failed; original error and recovery resource retained")
    }
}

impl<T> Error for ContractTempFailure<T> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.error.as_ref())
    }
}
