use super::{
    Evaluator, ControlFlow, LoweredValue, NativeArgumentValues, RuntimeError, Span, auth_module,
    lowered_bool_arg_or, lowered_int_arg, lowered_path_arg, lowered_record_arg,
    lowered_result_err_value, lowered_result_ok, lowered_runtime_result,
    lowered_session_user_record, lowered_str_arg_owned, lowered_str_list_arg, record_path,
    user_module,
};

impl Evaluator {
    pub(super) fn eval_lowered_applet_su_session_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let extra_args = lowered_str_list_arg(values.pop(), "applet.su_session", span)?;
            let command = lowered_str_arg_owned(values.pop(), "", "applet.su_session", span)?;
            let shell = lowered_str_arg_owned(values.pop(), "", "applet.su_session", span)?;
            let preserve_env =
                lowered_bool_arg_or(values.pop(), false, "applet.su_session", span)?;
            let login = lowered_bool_arg_or(values.pop(), false, "applet.su_session", span)?;
            let record = lowered_record_arg(values.pop(), "applet.su_session", span)?;
            let home = self.host_path(&record_path(&record, "home", span)?);
            let user = lowered_session_user_record(record, home, span)?;
            match auth_module::su_session(
                &user,
                login,
                preserve_env,
                &shell,
                &command,
                &extra_args,
            ) {
                Ok(code) => lowered_result_ok(LoweredValue::Int(i64::from(code))),
                Err(error) => lowered_result_err_value(
                    RuntimeError::host("applet.su_session", &error).with_span(span),
                ),
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_user_add_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let gecos = lowered_str_arg_owned(values.get(5).cloned(), "", "user.add", span)?;
            let shell = match values.get(4).cloned() {
                Some(value) => Some(lowered_path_arg(value, "user.add", span)?),
                None => None,
            };
            let home = match values.get(3).cloned() {
                Some(value) => Some(lowered_path_arg(value, "user.add", span)?),
                None => None,
            };
            let gid = match values.get(2).cloned() {
                Some(value) => Some(lowered_int_arg(Some(value), "user.add", span)?),
                None => None,
            };
            let uid = match values.get(1).cloned() {
                Some(value) => Some(lowered_int_arg(Some(value), "user.add", span)?),
                None => None,
            };
            let name = lowered_str_arg_owned(values.first().cloned(), "", "user.add", span)?;
            lowered_runtime_result(
                user_module::add(&name, uid, gid, home, shell, &gecos, span),
                span,
            )?
        };
        Ok(ControlFlow::Continue(value))
    }
}
