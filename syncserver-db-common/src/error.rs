use std::fmt;

use backtrace::Backtrace;
use http::StatusCode;
use syncserver_common::{from_error, impl_fmt_display, ReportableError};
use thiserror::Error;

/// Error specific to any MySQL database backend. These errors are not related to the syncstorage
/// or tokenserver application logic; rather, they are lower-level errors arising from diesel.
#[derive(Debug)]
pub struct MysqlError {
    kind: MysqlErrorKind,
    pub status: StatusCode,
    pub backtrace: Backtrace,
}

#[derive(Debug, Error)]
enum MysqlErrorKind {
    #[error("A database error occurred: {}", _0)]
    DieselQuery(#[from] diesel::result::Error),

    #[error("An error occurred while establishing a db connection: {}", _0)]
    DieselConnection(#[from] diesel::result::ConnectionError),

    #[error("A database pool error occurred: {}", _0)]
    Pool(diesel::r2d2::PoolError),

    #[error("Error migrating the database: {}", _0)]
    Migration(diesel_migrations::RunMigrationsError),
}

impl From<MysqlErrorKind> for MysqlError {
    fn from(kind: MysqlErrorKind) -> Self {
        Self {
            kind,
            status: StatusCode::INTERNAL_SERVER_ERROR,
            backtrace: Backtrace::new(),
        }
    }
}

impl ReportableError for MysqlError {
    fn is_sentry_event(&self) -> bool {
        #[allow(clippy::match_like_matches_macro)]
        match &self.kind {
            MysqlErrorKind::Pool(_) => false,
            _ => true,
        }
    }

    fn metric_label(&self) -> Option<String> {
        Some(
            match self.kind {
                MysqlErrorKind::DieselQuery(_) => "storage.mysql.error.diesel_query",
                MysqlErrorKind::DieselConnection(_) => "storage.mysql.error.diesel_connection",
                MysqlErrorKind::Pool(_) => "storage.mysql.error.pool",
                MysqlErrorKind::Migration(_) => "storage.mysql.error.migration",
            }
            .to_string(),
        )
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        Some(&self.backtrace)
    }
}

impl_fmt_display!(MysqlError, MysqlErrorKind);

from_error!(
    diesel::result::Error,
    MysqlError,
    MysqlErrorKind::DieselQuery
);
from_error!(
    diesel::result::ConnectionError,
    MysqlError,
    MysqlErrorKind::DieselConnection
);
from_error!(diesel::r2d2::PoolError, MysqlError, MysqlErrorKind::Pool);
from_error!(
    diesel_migrations::RunMigrationsError,
    MysqlError,
    MysqlErrorKind::Migration
);
