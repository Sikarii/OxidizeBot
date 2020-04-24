use crate::{auth, command, module, prelude::*, utils::Duration};
use anyhow::Error;
use chrono::Utc;

/// Handler for the !auth command.
pub struct Handler {
    auth: auth::Auth,
}

#[async_trait]
impl command::Handler for Handler {
    async fn handle(&self, ctx: &mut command::Context) -> Result<(), Error> {
        match ctx.next().as_deref() {
            Some("scopes") => {
                let filter = ctx.next();
                let filter = filter.as_deref();

                let user = match ctx.user.real() {
                    Some(user) => user,
                    None => {
                        respond!(ctx, "Can only get scopes for real users");
                        return Ok(());
                    }
                };

                // apply the current filter to a collection of scopes.
                let filter = |list: Vec<auth::Scope>| {
                    list.into_iter()
                        .map(|s| s.to_string())
                        .filter(|s| filter.map(|f| s.contains(f)).unwrap_or(true))
                        .collect::<Vec<_>>()
                };

                let by_user = filter(self.auth.scopes_for_user(user.name()).await);

                let mut result = Vec::new();

                if !by_user.is_empty() {
                    result.push(format!(
                        "Your ({}): {}",
                        user.display_name(),
                        by_user.join(", ")
                    ));
                }

                for role in user.roles() {
                    let by_role = filter(self.auth.scopes_for_role(role).await);

                    if !by_role.is_empty() {
                        result.push(format!("{}: {}", role, by_role.join(", ")));
                    }
                }

                if result.is_empty() {
                    respond!(ctx, "*no scopes*");
                    return Ok(());
                }

                respond!(ctx, format!("{}.", result.join("; ")));
            }
            Some("permit") => {
                ctx.check_scope(auth::Scope::AuthPermit).await?;

                let duration: Duration = ctx.next_parse("<duration> <principal> <scope>")?;
                let principal = ctx.next_parse("<duration> <principal> <scope>")?;
                let scope = ctx.next_parse("<duration> <principal> <scope>")?;

                if !ctx.user.has_scope(scope).await {
                    respond!(
                        ctx,
                        "Trying to grant scope `{}` that you don't have :(",
                        scope
                    );
                    return Ok(());
                }

                let now = Utc::now();
                let expires_at = now + duration.as_chrono();

                respond!(
                    ctx,
                    "Gave: {scope} to {principal} for {duration}",
                    duration = duration,
                    principal = principal,
                    scope = scope
                );

                self.auth
                    .insert_temporary(scope, principal, expires_at)
                    .await;
            }
            _ => {
                respond!(ctx, "Expected: permit");
            }
        }

        Ok(())
    }
}

pub struct Module;

impl Module {
    pub fn load() -> Self {
        Module
    }
}

#[async_trait]
impl super::Module for Module {
    fn ty(&self) -> &'static str {
        "auth"
    }

    async fn hook(
        &self,
        module::HookContext { handlers, auth, .. }: module::HookContext<'_>,
    ) -> Result<(), Error> {
        handlers.insert("auth", Handler { auth: auth.clone() });
        Ok(())
    }
}
