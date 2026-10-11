use super::{
    Evaluator, ControlFlow, LoweredValue, NativeArgumentValues, RuntimeError, Span, cli_module,
    lowered_module_result_value, lowered_record_runtime_arg, lowered_str_arg_owned,
    lowered_str_list_runtime_arg,
};

impl Evaluator {
    pub(super) fn eval_lowered_cli_commands_values(
        &mut self, mut values: NativeArgumentValues, span: Span, cli_plan: Option<&crate::modules::cli::CliDescriptorPlan>,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let argv = lowered_str_list_runtime_arg(values.remove(0), "cli.commands", span)?;
            let (rootless_default, commands, fallback_command) = if values.len() == 1 {
                (
                    String::new(),
                    lowered_record_runtime_arg(values.remove(0), "cli.commands", span)?,
                    None,
                )
            } else {
                let rootless_default =
                    lowered_str_arg_owned(Some(values.remove(0)), "", "cli.commands", span)?;
                let commands =
                    lowered_record_runtime_arg(values.remove(0), "cli.commands", span)?;
                let fallback_command = match values.pop() {
                    Some(value) => {
                        Some(lowered_record_runtime_arg(value, "cli.commands", span)?)
                    }
                    None => None,
                };
                (rootless_default, commands, fallback_command)
            };
            lowered_module_result_value(
                cli_module::parse_commands(
                    argv,
                    rootless_default,
                    commands,
                    fallback_command,
                    span,
                    cli_plan,
                ),
                span,
            )?
        };
        Ok(ControlFlow::Continue(value))
    }
}
