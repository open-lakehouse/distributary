//! What the SQL stores share: checking an applied schema against the
//! migrations embedded in this binary.

use sqlx::migrate::Migrator;

use crate::{Error, Result};

/// One row of sqlx's `_sqlx_migrations`: version, checksum, success.
pub(crate) type AppliedMigration = (i64, Vec<u8>, bool);

/// Fail unless `applied` is exactly `migrator`'s migrations: none pending,
/// none dirty, none modified, none unknown.
pub(crate) fn check_applied(migrator: &Migrator, applied: &[AppliedMigration]) -> Result<()> {
    if let Some((version, _, _)) = applied.iter().find(|(_, _, ok)| !ok) {
        return Err(Error::generic(format!(
            "migration {version} is dirty (failed part-way); repair it and re-run `migrate`"
        )));
    }
    let expected: Vec<_> = migrator
        .iter()
        .filter(|m| !m.migration_type.is_down_migration())
        .collect();
    for migration in &expected {
        match applied.iter().find(|(v, _, _)| *v == migration.version) {
            None => {
                return Err(Error::generic(format!(
                    "migration {} ({}) is pending; run `sharing-server migrate`",
                    migration.version, migration.description
                )));
            }
            Some((_, checksum, _)) if checksum.as_slice() != &*migration.checksum => {
                return Err(Error::generic(format!(
                    "migration {} was modified after it was applied",
                    migration.version
                )));
            }
            Some(_) => {}
        }
    }
    if let Some((version, _, _)) = applied
        .iter()
        .find(|(v, _, _)| !expected.iter().any(|m| m.version == *v))
    {
        return Err(Error::generic(format!(
            "the database has migration {version}, unknown to this binary (is it newer?)"
        )));
    }
    Ok(())
}

/// The query listing applied migrations.
pub(crate) const APPLIED_MIGRATIONS: &str =
    "SELECT version, checksum, success FROM _sqlx_migrations ORDER BY version";
