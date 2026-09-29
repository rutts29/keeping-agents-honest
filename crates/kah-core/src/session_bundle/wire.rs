use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WireBundle {
    pub(super) schema_version: u32,
    pub(super) source: WireSource,
    pub(super) native_goal: NativeGoal,
    pub(super) records: Vec<WireRecord>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WireSource {
    pub(super) provider: SourceProvider,
    pub(super) session_ref: String,
    pub(super) repository_digest: String,
    pub(super) head: String,
    pub(super) digest: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WireRecord {
    pub(super) id: String,
    pub(super) role: SourceRole,
    pub(super) content: String,
    pub(super) content_digest: String,
}

impl<'de> Deserialize<'de> for NeutralSessionBundle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = WireBundle::deserialize(deserializer)?;
        NeutralSessionBundle::try_from(wire).map_err(serde::de::Error::custom)
    }
}

impl TryFrom<WireBundle> for NeutralSessionBundle {
    type Error = SessionBundleError;

    fn try_from(wire: WireBundle) -> Result<Self, Self::Error> {
        if wire.schema_version != SESSION_BUNDLE_SCHEMA_VERSION {
            return Err(SessionBundleError::UnsupportedSchema(wire.schema_version));
        }
        wire.native_goal.validate()?;
        validate_identifier(&wire.source.session_ref, "source session_ref")?;
        validate_sha256(&wire.source.repository_digest, "source repository_digest")?;
        validate_identifier(&wire.source.head, "source head")?;
        validate_sha256(&wire.source.digest, "source digest")?;

        let records = wire
            .records
            .into_iter()
            .map(|record| {
                validate_identifier(&record.id, "record id")?;
                validate_content(&record.content)?;
                if record.content != normalize_content(record.content.clone()) {
                    return Err(SessionBundleError::InvalidField(
                        "record content must be normalized",
                    ));
                }
                validate_sha256(&record.content_digest, "record content_digest")?;
                if record.content_digest != record_digest(&record.id, &record.role, &record.content)
                {
                    return Err(SessionBundleError::InvalidDigest("record content_digest"));
                }
                Ok(BundleRecord {
                    id: record.id,
                    role: record.role,
                    content: record.content,
                    content_digest: record.content_digest,
                })
            })
            .collect::<Result<Vec<_>, SessionBundleError>>()?;
        validate_records(&records)?;
        if wire.source.head != records.last().expect("validated non-empty records").id {
            return Err(SessionBundleError::SourceHeadMismatch);
        }
        if wire.source.digest != source_digest(&records) {
            return Err(SessionBundleError::SourceDigestMismatch);
        }

        Ok(Self {
            schema_version: wire.schema_version,
            source: BundleSource {
                provider: wire.source.provider,
                session_ref: wire.source.session_ref,
                repository_digest: wire.source.repository_digest,
                head: wire.source.head,
                digest: wire.source.digest,
            },
            native_goal: wire.native_goal,
            records,
        })
    }
}
