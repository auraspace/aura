//! C-only compatibility state nodes.
//!
//! This is intentionally not part of aura-ir: its payloads are rendered C
//! fragments and exist only while the alpha backend migrates to MIR.

use std::fmt::Write as _;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AsyncCfgNode {
    Action {
        code: String,
        next: usize,
    },
    Branch {
        condition: String,
        then_state: usize,
        else_state: usize,
    },
    Await {
        value: String,
        value_key: String,
        operand: String,
        owns_task: bool,
        next: usize,
    },
    AwaitUnit {
        operand: String,
        owns_task: bool,
        next: usize,
    },
    AwaitCatch {
        operand: String,
        owns_task: bool,
        catch_name: String,
        catch_key: String,
        catch_state: usize,
        failure_state: Option<usize>,
        finally_state: Option<usize>,
        next: usize,
    },
    AwaitCatchValue {
        value: String,
        value_key: String,
        operand: String,
        owns_task: bool,
        catch_name: String,
        catch_key: String,
        catch_state: usize,
        failure_state: Option<usize>,
        finally_state: Option<usize>,
        next: usize,
    },
    AwaitFinally {
        operand: String,
        owns_task: bool,
        finally_state: usize,
        next: usize,
    },
    Fail,
    Cancel,
    Return {
        value: String,
        value_key: String,
        value_is_ident: bool,
        value_is_owned_temp: bool,
    },
    Throw {
        value: String,
        value_key: String,
        span_start: u32,
        span_end: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AsyncFrameField {
    pub name: String,
    pub type_key: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AsyncStateMachine {
    pub frame_fields: Vec<AsyncFrameField>,
    pub nodes: Vec<Option<AsyncCfgNode>>,
}

impl AsyncStateMachine {
    pub fn validate_edges(&self) -> bool {
        self.nodes.iter().all(|node| {
            let Some(node) = node else { return false };
            let valid = |target: usize| target < self.nodes.len();
            match node {
                AsyncCfgNode::Action { next, .. }
                | AsyncCfgNode::Await { next, .. }
                | AsyncCfgNode::AwaitUnit { next, .. }
                | AsyncCfgNode::AwaitCatch { next, .. }
                | AsyncCfgNode::AwaitCatchValue { next, .. }
                | AsyncCfgNode::AwaitFinally { next, .. } => valid(*next),
                AsyncCfgNode::Branch {
                    then_state,
                    else_state,
                    ..
                } => valid(*then_state) && valid(*else_state),
                AsyncCfgNode::Return { .. }
                | AsyncCfgNode::Throw { .. }
                | AsyncCfgNode::Fail
                | AsyncCfgNode::Cancel => true,
            }
        })
    }

    pub fn dump_comments(&self, out: &mut String) {
        let _ = writeln!(
            out,
            "/* aura async model version=1 states={} */",
            self.nodes.len()
        );
        out.push_str("/* aura async frame fields:");
        for field in &self.frame_fields {
            let _ = write!(out, " {}:{}", field.name, field.type_key);
        }
        out.push_str(" */\n");
        for (state, node) in self.nodes.iter().enumerate() {
            let Some(node) = node else {
                let _ = writeln!(out, "/* aura async state={state} kind=invalid */");
                continue;
            };
            match node {
                AsyncCfgNode::Action { next, .. } => {
                    let _ = writeln!(
                        out,
                        "/* aura async state={state} kind=action next={next} */"
                    );
                }
                AsyncCfgNode::Branch {
                    then_state,
                    else_state,
                    ..
                } => {
                    let _ = writeln!(out, "/* aura async state={state} kind=branch then={then_state} else={else_state} */");
                }
                AsyncCfgNode::Await {
                    next, owns_task, ..
                }
                | AsyncCfgNode::AwaitUnit {
                    next, owns_task, ..
                } => {
                    let _ = writeln!(out, "/* aura async state={state} kind=await next={next} owns_task={owns_task} */");
                }
                AsyncCfgNode::AwaitCatch {
                    next,
                    catch_state,
                    owns_task,
                    ..
                }
                | AsyncCfgNode::AwaitCatchValue {
                    next,
                    catch_state,
                    owns_task,
                    ..
                } => {
                    let _ = writeln!(out, "/* aura async state={state} kind=await-catch next={next} catch={catch_state} owns_task={owns_task} */");
                }
                AsyncCfgNode::AwaitFinally {
                    next,
                    finally_state,
                    owns_task,
                    ..
                } => {
                    let _ = writeln!(out, "/* aura async state={state} kind=await-finally next={next} finally={finally_state} owns_task={owns_task} */");
                }
                AsyncCfgNode::Fail => {
                    let _ = writeln!(out, "/* aura async state={state} kind=fail */");
                }
                AsyncCfgNode::Cancel => {
                    let _ = writeln!(out, "/* aura async state={state} kind=cancel */");
                }
                AsyncCfgNode::Return { .. } => {
                    let _ = writeln!(out, "/* aura async state={state} kind=return */");
                }
                AsyncCfgNode::Throw {
                    span_start,
                    span_end,
                    ..
                } => {
                    let _ = writeln!(
                        out,
                        "/* aura async state={state} kind=throw span={span_start}:{span_end} */"
                    );
                }
            }
        }
    }
}
