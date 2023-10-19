pub use sea_orm_migration::prelude::*;

mod m20221010_000001_create_table;
mod m20221010_000002_create_table;
mod m20221010_000003_create_table;
mod m20221010_000004_create_table;
mod m20221010_000005_create_table;
mod m20221010_000006_create_trigger;
mod m20230110_000001_update_table;
mod m20230110_000002_create_table;
mod m20230210_000001_update_table;
mod m20230210_000002_update_table;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20221010_000001_create_table::Migration),
            Box::new(m20221010_000002_create_table::Migration),
            Box::new(m20221010_000003_create_table::Migration),
            Box::new(m20221010_000004_create_table::Migration),
            Box::new(m20221010_000005_create_table::Migration),
            Box::new(m20221010_000006_create_trigger::Migration),
            Box::new(m20230110_000001_update_table::Migration),
            Box::new(m20230110_000002_create_table::Migration),
            Box::new(m20230210_000001_update_table::Migration),
            Box::new(m20230210_000002_update_table::Migration),
        ]
    }
}
