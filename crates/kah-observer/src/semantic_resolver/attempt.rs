use super::*;

pub(super) struct Attempt<'a> {
    config: CompactorConfig<'a>,
    version: u32,
    started: Instant,
    calls: u8,
    usage: ResolverUsage,
    pub(super) failure: Option<String>,
    artifacts: Vec<String>,
}
impl<'a> Attempt<'a> {
    pub(super) fn new(config: CompactorConfig<'a>, version: u32) -> Self {
        Self {
            config,
            version,
            started: Instant::now(),
            calls: 0,
            usage: ResolverUsage::default(),
            failure: None,
            artifacts: Vec::new(),
        }
    }
    pub(super) fn record(
        &mut self,
        call: Result<CodexCall, ExecutionFailure>,
    ) -> Result<CodexCall, ResolverFailure> {
        self.calls += 1;
        match call {
            Ok(call) => {
                self.artifacts.push(call.artifact_id.clone());
                self.usage.add(&call.usage);
                Ok(call)
            }
            Err(error) => {
                self.artifacts.extend(error.artifact);
                let mut failure = self.error(ResolverFailureKind::Execution);
                failure.code = error.code.into();
                failure.stage = error.stage;
                failure.metadata.usage_complete = false;
                Err(failure)
            }
        }
    }
    fn metadata(&self) -> ResolverMetadata {
        metadata_for_prompt(
            self.config,
            self.calls,
            self.started.elapsed().as_millis(),
            self.usage.clone(),
            self.failure.clone(),
            &self.artifacts,
            self.version,
        )
    }
    pub(super) fn error(&self, kind: ResolverFailureKind) -> ResolverFailure {
        ResolverFailure {
            code: self
                .failure
                .clone()
                .unwrap_or_else(|| "OBSERVER_EXECUTION".into()),
            stage: "proposal_validation",
            kind,
            metadata: Box::new(self.metadata()),
        }
    }
    pub(super) fn accept(
        &self,
        history: History,
        mut projection: ActiveProjection,
        native_goal: NativeGoalObservation,
        goal_change: Option<GoalChangeObservation>,
    ) -> InspectResolution {
        projection.generated_by.model = Some(self.config.model().to_owned());
        projection.generated_by.reasoning = Some(self.config.reasoning().to_owned());
        projection.generated_by.prompt_schema_version = self.version;
        InspectResolution {
            history,
            projection,
            metadata: self.metadata(),
            native_goal,
            goal_change,
        }
    }
}
