use super::message::{CauseBy, Message};
use super::memory::Memory;
use super::environment::Environment;
use super::context_manager::ContextManager;
use super::role_context::RoleContext;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub enum ReactMode {
    /// Single observe→think→act→publish pass
    React,
    /// Execute actions in sequential order (by_order)
    ByOrder,
}

pub struct Role {
    pub name: String,
    pub profile: String,
    pub goal: String,
    pub constraints: String,
    pub actions: Vec<Box<dyn super::action::Action>>,
    pub watch: Vec<CauseBy>,
    pub memory: Memory,
    pub is_idle: bool,
    pub context_manager: ContextManager,
    pub max_react_loop: usize,
    pub react_mode: ReactMode,
    pub rc: Option<Arc<RoleContext>>,
}

impl Role {
    pub fn new(name: &str, profile: &str, goal: &str, constraints: &str) -> Self {
        Self {
            name: name.to_string(),
            profile: profile.to_string(),
            goal: goal.to_string(),
            constraints: constraints.to_string(),
            actions: Vec::new(),
            watch: Vec::new(),
            memory: Memory::new(),
            is_idle: true,
            context_manager: ContextManager::default(),
            max_react_loop: 5,
            react_mode: ReactMode::React,
            rc: None,
        }
    }

    pub fn with_react_mode(mut self, mode: ReactMode) -> Self { self.react_mode = mode; self }
    pub fn with_rc(mut self, rc: Arc<RoleContext>) -> Self { self.rc = Some(rc); self }

    pub fn add_action(&mut self, action: Box<dyn super::action::Action>) { self.actions.push(action); }
    pub fn watch(&mut self, causes: Vec<CauseBy>) { self.watch = causes; }

    pub fn put_message(&mut self, msg: Message) {
        self.memory.add(msg);
        self.is_idle = false;
    }

    pub fn observe(&self, env: &Environment) -> bool {
        let msgs = env.peek_messages(&self.name);
        !msgs.is_empty()
    }

    pub fn think(&mut self, env: &mut Environment) -> bool {
        let msgs = env.get_messages(&self.name);
        if msgs.is_empty() {
            self.is_idle = true;
            return false;
        }
        self.memory.add_batch(msgs);
        self.is_idle = false;
        true
    }

    pub async fn act(&mut self, provider: &crate::native_engine::provider_manager::ResolvedProvider) -> Result<String, String> {
        let context = self.context_manager.build_context(
            &self.memory.get_all().iter().collect::<Vec<_>>()
        );
        let mut last_output = String::new();

        match self.react_mode {
            ReactMode::ByOrder => {
                // Execute all actions in sequence, accumulating context
                let mut accumulated = context.clone();
                for action in &self.actions {
                    match action.run(&accumulated, provider).await {
                        Ok(output) => {
                            accumulated = format!("{}\n\n{}", accumulated, output);
                            last_output = output;
                        }
                        Err(e) => {
                            tracing::error!(target: "metagpt", "Role '{}' action '{}' failed: {}", self.name, action.name(), e);
                            return Err(e.to_string());
                        }
                    }
                }
            }
            ReactMode::React => {
                // Single action pass
                for action in &self.actions {
                    match action.run(&context, provider).await {
                        Ok(output) => { last_output = output; }
                        Err(e) => {
                            tracing::error!(target: "metagpt", "Role '{}' action '{}' failed: {}", self.name, action.name(), e);
                            return Err(e.to_string());
                        }
                    }
                }
            }
        }
        Ok(last_output)
    }

    pub fn publish(&self, env: &mut Environment, output: &str) {
        for action in &self.actions {
            let msg = action.to_message(output, &self.name);
            env.publish_message(msg);
        }
    }

    pub async fn run(&mut self, env: &mut Environment, provider: &crate::native_engine::provider_manager::ResolvedProvider) -> Result<(), String> {
        for loop_iter in 0..self.max_react_loop {
            let observed = self.observe(env);
            tracing::info!(target: "metagpt", "[{}] loop_iter={} observed={} memory={}", self.name, loop_iter, observed, self.memory.get_all().len());
            if !observed && loop_iter > 0 { break; }
            if !self.think(env) {
                tracing::warn!(target: "metagpt", "[{}] think returned false at iter {}", self.name, loop_iter);
                break;
            }
            tracing::info!(target: "metagpt", "[{}] calling act(), memory msgs={}", self.name, self.memory.get_all().len());
            match self.act(provider).await {
                Ok(output) => {
                    tracing::info!(target: "metagpt", "[{}] act() succeeded, output={} chars, publishing...", self.name, output.len());
                    self.publish(env, &output);
                    tracing::info!(target: "metagpt", "[{}] published to env, history={} msgs", self.name, env.history.get_all().len());
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!(target: "metagpt", "Role '{}' loop {} failed: {}", self.name, loop_iter, e);
                    // Publish partial context so downstream roles can continue
                    let partial = self.context_manager.build_context(
                        &self.memory.get_all().iter().collect::<Vec<_>>()
                    );
                    if !partial.is_empty() {
                        tracing::info!(target: "metagpt", "[{}] publishing partial context ({} chars) despite failure", self.name, partial.len());
                        self.publish(env, &partial);
                    }
                    return Err(e);
                }
            }
        }
        tracing::warn!(target: "metagpt", "[{}] run() exiting without publishing (is_idle)", self.name);
        self.is_idle = true;
        Ok(())
    }
}
