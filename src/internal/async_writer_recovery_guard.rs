// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Transfers a live asynchronous writer back to run evidence when abandoned.

use qubit_fs::write::AsyncFileWriter;

use crate::ContractCheckId;
use crate::ContractFailure;
use crate::ContractWriterFailure;

/// Owns a writer throughout every asynchronous write and abort suspension.
pub(crate) struct AsyncWriterRecoveryGuard<'a> {
    writer: Option<AsyncFileWriter>,
    failures: &'a mut Vec<ContractFailure>,
    id: ContractCheckId,
    interruption: &'static str,
}

impl<'a> AsyncWriterRecoveryGuard<'a> {
    /// Arms ownership immediately after a writer has been opened.
    pub(crate) fn new(
        writer: AsyncFileWriter,
        failures: &'a mut Vec<ContractFailure>,
        id: ContractCheckId,
        interruption: &'static str,
    ) -> Self {
        Self {
            writer: Some(writer),
            failures,
            id,
            interruption,
        }
    }

    /// Borrows the writer while an asynchronous operation is active.
    pub(crate) fn writer_mut(&mut self) -> &mut AsyncFileWriter {
        self.writer.as_mut().expect("writer guard retains its writer")
    }

    /// Transfers the writer after the operation has resolved.
    pub(crate) fn take(&mut self) -> AsyncFileWriter {
        self.writer.take().expect("writer guard transfers its writer once")
    }
}

impl Drop for AsyncWriterRecoveryGuard<'_> {
    /// Retains writer ownership synchronously without provider I/O.
    fn drop(&mut self) {
        if let Some(writer) = self.writer.take() {
            self.failures.push(
                ContractFailure::with_owned_source(
                    self.interruption,
                    ContractWriterFailure::new(ContractFailure::message_only(self.interruption).at(self.id), writer),
                )
                .at(self.id),
            );
        }
    }
}
