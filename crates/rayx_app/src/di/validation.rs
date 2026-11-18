//! What an app's registrations require, checked before anything is made: the traits the app (or
//! its host) needs, and the traits each registration depends on.
//!
//! A registration declares what it depends on ([`ServiceProvider::require_for`]) and the app or
//! its host what it needs ([`ServiceProvider::require`]), each with a hint saying how to register
//! it. [`ServiceProvider::validate`], which `RayXAppBuilder::build` and `build_async` run first,
//! fails with one [`ServiceValidationError`] listing every missing service and every singleton
//! that depends on a scoped one.

use std::borrow::Cow;
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use super::key::ServiceKey;
use super::registry::ServiceLifetime;

/// One requirement: `needs` (with its name), required by the app (`by: None`) or by the
/// registration `by`, and how to register it.
#[derive(Clone, Debug)]
pub(crate) struct Requirement {
    pub(crate) needs: ServiceKey,
    pub(crate) needs_name: Cow<'static, str>,
    pub(crate) by: Option<(ServiceKey, Cow<'static, str>)>,
    pub(crate) hint: Cow<'static, str>,
    /// A registration's requirement belongs to a registration of `by` already (the first one
    /// after it was declared); until then it waits for one.
    pub(crate) attached: bool,
}

/// The requirements of a provider and its clones and scopes.
#[derive(Clone, Debug, Default)]
pub(crate) struct Requirements(Arc<Mutex<Vec<Requirement>>>);

impl Requirements {
    pub(crate) fn push(&self, requirement: Requirement) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(requirement);
    }

    /// What registering `key` does: the requirements the registration it replaces declared go,
    /// and those declared for `key` since (before this registration) are this one's.
    pub(crate) fn registered(&self, key: &ServiceKey) {
        let mut requirements = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let of_key =
            |requirement: &Requirement| requirement.by.as_ref().is_some_and(|(by, _)| by == key);
        requirements.retain(|requirement| !(of_key(requirement) && requirement.attached));
        for requirement in requirements
            .iter_mut()
            .filter(|requirement| of_key(requirement))
        {
            requirement.attached = true;
        }
    }

    pub(crate) fn all(&self) -> Vec<Requirement> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// What is wrong with an app's registrations: one entry per problem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServiceValidationProblem {
    /// Nothing is registered as `needed`, which `by` (the app when `None`) requires; `hint` says
    /// how to register it.
    Missing {
        needed: String,
        by: Option<String>,
        hint: String,
    },
    /// The singleton `singleton` depends on `scoped`, a scoped service, which a singleton
    /// outlives: directly, or `through` transient services (each made when the one before it
    /// is, so the singleton keeps what they resolved); `hint` is the last dependency's.
    SingletonOnScoped {
        singleton: String,
        through: Vec<String>,
        scoped: String,
        hint: String,
    },
}

impl fmt::Display for ServiceValidationProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing {
                needed,
                by: Some(by),
                hint,
            } => write!(
                f,
                "`{by}` needs `{needed}`, which is not registered: {hint}"
            ),
            Self::Missing {
                needed,
                by: None,
                hint,
            } => write!(
                f,
                "the app needs `{needed}`, which is not registered: {hint}"
            ),
            Self::SingletonOnScoped {
                singleton,
                through,
                scoped,
                hint,
            } => {
                write!(f, "`{singleton}` is a singleton and needs ")?;
                for transient in through {
                    write!(f, "`{transient}` (transient), which needs ")?;
                }
                write!(
                    f,
                    "`{scoped}`, which is scoped (a singleton outlives every scope): register \
                     `{scoped}` as a singleton, or `{singleton}` as scoped ({hint})"
                )
            }
        }
    }
}

/// Why an app's registrations are not complete: every problem at once, one per line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceValidationError {
    pub problems: Vec<ServiceValidationProblem>,
}

impl fmt::Display for ServiceValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the app's services are not complete:")?;
        for problem in &self.problems {
            write!(f, "\n- {problem}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ServiceValidationError {}

/// The lifetime a service is registered with, from its key's registration.
pub(crate) type LifetimeOf<'a> = dyn Fn(&ServiceKey) -> Option<ServiceLifetime> + 'a;

/// Checks `requirements` against what is registered (`lifetime_of` a key, `None` when nothing
/// is): every missing service, and every singleton that depends on a scoped service, directly or
/// through transient ones.
pub(crate) fn validate(
    requirements: &[Requirement],
    lifetime_of: &LifetimeOf<'_>,
) -> Result<(), ServiceValidationError> {
    let mut problems = Vec::new();
    let mut push = |problem: ServiceValidationProblem| {
        if !problems.contains(&problem) {
            problems.push(problem);
        }
    };
    for requirement in requirements {
        if lifetime_of(&requirement.needs).is_none() {
            push(ServiceValidationProblem::Missing {
                needed: requirement.needs_name.to_string(),
                by: requirement.by.as_ref().map(|(_, name)| name.to_string()),
                hint: requirement.hint.to_string(),
            });
            continue;
        }
        if let Some((by, name)) = &requirement.by
            && lifetime_of(by) == Some(ServiceLifetime::Singleton)
        {
            let mut visited = vec![by.clone()];
            captive(
                requirements,
                lifetime_of,
                requirement,
                name,
                &mut Vec::new(),
                &mut visited,
                &mut push,
            );
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(ServiceValidationError { problems })
    }
}

/// Reports `singleton` keeping a scoped service through `requirement`: the scoped service it
/// names, or one a transient service it names depends on (followed through `requirements`).
fn captive(
    requirements: &[Requirement],
    lifetime_of: &LifetimeOf<'_>,
    requirement: &Requirement,
    singleton: &str,
    through: &mut Vec<String>,
    visited: &mut Vec<ServiceKey>,
    push: &mut impl FnMut(ServiceValidationProblem),
) {
    match lifetime_of(&requirement.needs) {
        Some(ServiceLifetime::Scoped) => push(ServiceValidationProblem::SingletonOnScoped {
            singleton: singleton.to_owned(),
            through: through.clone(),
            scoped: requirement.needs_name.to_string(),
            hint: requirement.hint.to_string(),
        }),
        Some(ServiceLifetime::Transient) if !visited.contains(&requirement.needs) => {
            visited.push(requirement.needs.clone());
            through.push(requirement.needs_name.to_string());
            for next in requirements.iter().filter(|next| {
                next.by
                    .as_ref()
                    .is_some_and(|(by, _)| *by == requirement.needs)
            }) {
                captive(
                    requirements,
                    lifetime_of,
                    next,
                    singleton,
                    through,
                    visited,
                    push,
                );
            }
            through.pop();
        }
        _ => {}
    }
}
