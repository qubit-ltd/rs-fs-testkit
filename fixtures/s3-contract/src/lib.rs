mod config;
mod error_mapper;
mod path_mapper;
mod s3_directory_stream;
mod s3_file_system_spi;
mod s3_reader;
mod s3_write_session;

pub use config::S3ContractConfig;
pub use path_mapper::map;
pub use path_mapper::validate_key;
pub use s3_file_system_spi::open;
pub use s3_file_system_spi::open_in_memory;
pub use s3_file_system_spi::open_with_store;

mod test_control;
pub use s3_file_system_spi::open_with_control;
pub use test_control::TestControl;
pub use test_control::TestStage;
