use super::*;

// Oxc walks nested patterns; expression values use the ordinary DF lift once.
pub(super) struct DefaultValues<'s> {
    pub(super) file: &'s str,
    pub(super) fn_sym: &'s DfOwner,
    pub(super) strings: &'s mut Strings,
    pub(super) scope: &'s mut Scope,
    pub(super) sink: &'s mut FamilyBundle<DfF>,
}

impl<'a> OxcVisit<'a> for DefaultValues<'_> {
    fn visit_expression(&mut self, _expression: &ts::Expression<'a>) {}

    fn visit_assignment_pattern(&mut self, pattern: &ts::AssignmentPattern<'a>) {
        df_flow_expr(
            &pattern.right,
            self.file,
            self.fn_sym,
            self.strings,
            self.scope,
            self.sink,
        );
        self.visit_binding_pattern(&pattern.left);
    }

    fn visit_assignment_target_with_default(
        &mut self,
        target: &ts::AssignmentTargetWithDefault<'a>,
    ) {
        df_flow_expr(
            &target.init,
            self.file,
            self.fn_sym,
            self.strings,
            self.scope,
            self.sink,
        );
        self.visit_assignment_target(&target.binding);
    }

    fn visit_assignment_target_property_identifier(
        &mut self,
        property: &ts::AssignmentTargetPropertyIdentifier<'a>,
    ) {
        if let Some(value) = &property.init {
            df_flow_expr(
                value,
                self.file,
                self.fn_sym,
                self.strings,
                self.scope,
                self.sink,
            );
        }
    }
}
