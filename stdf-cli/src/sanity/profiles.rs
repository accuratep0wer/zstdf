use super::*;
use std::collections::BTreeSet;

#[derive(Serialize)]
#[serde(untagged)]
pub(super) enum Profiles {
    Single(Profile),
    Routed(Routing),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Routing {
    version: u32,
    id: String,
    routes: Vec<Route>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Route {
    matches: Vec<Selector>,
    profile: Profile,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selector {
    source_id: Option<String>,
    /// String to avoid JSON/JavaScript integer precision loss.
    mir_offset: Option<String>,
    #[serde(default)]
    mir: BTreeMap<String, Value>,
}
impl Profiles {
    pub fn load(args: &Arguments) -> CliResult<Self> {
        if let Some(path) = &args.run_profiles {
            if args.test_domain.is_some() || args.profile.is_some() {
                return Err("--run-profiles conflicts with --test-domain/--profile".into());
            }
            if fs::metadata(path)?.len() > 1024 * 1024 {
                return Err("run profiles exceed 1 MiB".into());
            }
            let mut routing: Routing = serde_json::from_reader(File::open(path)?)?;
            if routing.version != 1 || routing.id.is_empty() || routing.routes.is_empty() {
                return Err("invalid run profiles version/id/routes".into());
            }
            let mut ids = BTreeSet::new();
            for route in &mut routing.routes {
                route.profile.validate()?;
                if !ids.insert(route.profile.id.clone()) || route.matches.is_empty() {
                    return Err("duplicate profile id or empty match groups".into());
                }
                for s in &route.matches {
                    if s.source_id.is_none() && s.mir_offset.is_none() && s.mir.is_empty() {
                        return Err("empty run selector is not allowed".into());
                    }
                    if s.source_id.as_ref().is_some_and(|h| {
                        h.len() != 64
                            || !h
                                .bytes()
                                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    }) {
                        return Err("source_id must be lowercase SHA-256 hex".into());
                    }
                    if let Some(offset) = &s.mir_offset {
                        let n: u64 = offset.parse()?;
                        if n.to_string() != *offset {
                            return Err(
                                "mir_offset must be a canonical unsigned decimal string".into()
                            );
                        }
                    }
                    for (name, value) in &s.mir {
                        if !fields::layout("MIR")
                            .unwrap()
                            .split_whitespace()
                            .any(|t| t.split(':').next() == Some(name))
                            || !matches!(value, Value::String(_) | Value::Number(_))
                        {
                            return Err(format!("invalid MIR selector {name}").into());
                        }
                    }
                }
            }
            Ok(Self::Routed(routing))
        } else {
            Ok(Self::Single(Profile::load(args)?))
        }
    }
    pub fn default_profile(&self) -> Option<&Profile> {
        match self {
            Self::Single(p) => Some(p),
            Self::Routed(_) => None,
        }
    }
    pub fn select(
        &self,
        source: &str,
        offset: usize,
        fields: &[Field],
    ) -> Result<&Profile, String> {
        match self {
            Self::Single(p) => Ok(p),
            Self::Routed(config) => {
                let matches: Vec<_> = config
                    .routes
                    .iter()
                    .filter(|r| {
                        r.matches.iter().any(|s| {
                            s.source_id.as_ref().is_none_or(|h| h == source)
                                && s.mir_offset
                                    .as_ref()
                                    .is_none_or(|o| o == &offset.to_string())
                                && s.mir.iter().all(|(name, value)| {
                                    fields::field(fields, name).is_some_and(|f| {
                                        f.status == "valid" && f.effective == *value
                                    })
                                })
                        })
                    })
                    .collect();
                match matches.as_slice() {
                    [route] => Ok(&route.profile),
                    [] => Err("run profile unmatched; product checks not evaluated".into()),
                    _ => Err(format!(
                        "run profile ambiguous: {}",
                        matches
                            .iter()
                            .map(|r| r.profile.id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                }
            }
        }
    }
}
