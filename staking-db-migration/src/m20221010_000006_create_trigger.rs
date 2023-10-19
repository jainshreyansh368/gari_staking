use sea_orm::{DbBackend, Statement};
use sea_orm_migration::{prelude::*, sea_orm::ConnectionTrait};

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20221010_000006_create_trigger"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        let function_statement =
            Statement::from_string(DbBackend::Postgres, CREATE_FUNCTION.to_string());

        match conn.execute(function_statement).await {
            Ok(_) => {
                let alter_statement =
                    Statement::from_string(DbBackend::Postgres, ALTER_FUNCTION.to_string());

                match conn.execute(alter_statement).await {
                    Ok(_) => {
                        let trigger_statement =
                            Statement::from_string(DbBackend::Postgres, CREATE_TRIGGER.to_string());
                        match conn.execute(trigger_statement).await {
                            Ok(_) => Ok(()),
                            Err(error) => Err(error),
                        }
                    }
                    Err(error) => Err(error),
                }
            }
            Err(error) => Err(error),
        }
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();
        let function_statement =
            Statement::from_string(DbBackend::Postgres, DROP_FUNCTION.to_string());

        match conn.execute(function_statement).await {
            Ok(_) => {
                let trigger_statement =
                    Statement::from_string(DbBackend::Postgres, DROP_TRIGGER.to_string());
                match conn.execute(trigger_statement).await {
                    Ok(_) => Ok(()),
                    Err(error) => Err(error),
                }
            }
            Err(error) => Err(error),
        }
    }
}

const CREATE_FUNCTION: &str = r#"CREATE OR REPLACE FUNCTION public.update_user_balance()
	    RETURNS trigger
	    LANGUAGE 'plpgsql'
	    COST 100
	    VOLATILE NOT LEAKPROOF
	AS $BODY$
	BEGIN
		IF NEW.instruction_type = 'stake' THEN
			UPDATE public.staking_user_data SET balance = balance + NEW.amount 
			WHERE user_spl_token_owner = NEW.user_spl_token_owner;
		END IF;
		IF NEW.instruction_type = 'unstake' THEN
			UPDATE public.staking_user_data SET balance = balance - NEW.amount 
			WHERE user_spl_token_owner = NEW.user_spl_token_owner;
		END IF;
		RETURN NEW;
	END;
	$BODY$;"#;

const ALTER_FUNCTION: &str = r#"ALTER FUNCTION public.update_user_balance() 
    OWNER TO postgres;"#;

const CREATE_TRIGGER: &str = r#"CREATE TRIGGER trg_update_user_balance
    AFTER INSERT
    ON public.staking_user_transaction_history
    FOR EACH ROW
    EXECUTE FUNCTION public.update_user_balance();"#;

const DROP_FUNCTION: &str = r#"DROP FUNCTION IF EXISTS public.update_user_balance();"#;

const DROP_TRIGGER: &str =
    r#"DROP TRIGGER IF EXISTS trg_update_user_balance ON public.staking_user_transaction_history;"#;
