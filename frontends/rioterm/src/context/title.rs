use crate::context::Context;

// TODO:
// Regex {{x}} entries, then validate if x contains "||"
// if it does split per "||" then evaluate first to last item (y)
// until y is not empty

#[inline]
fn update_title<T: rio_backend::event::EventListener>(
    template: String,
    context: &Context<T>,
) -> String {
    let mut template = template.to_owned();

    let columns = "{{COLUMNS}}";
    if template.contains(columns) {
        template = template.replace(columns, &context.dimension.columns.to_string());
    }

    let columns = "{{LINES}}";
    if template.contains(columns) {
        template = template.replace(columns, &context.dimension.columns.to_string());
    }

    template
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::context::create_mock_context;
    use crate::context::ContextDimension;
    use crate::context::Delta;
    use rio_backend::event::VoidListener;
    use rio_backend::sugarloaf::layout::SugarDimensions;
    use rio_window::window::WindowId;

    #[test]
    fn test_update_title() {
        let context_dimension = ContextDimension::build(
            1200.0,
            800.0,
            SugarDimensions {
                scale: 2.,
                width: 18.,
                height: 9.,
            },
            1.0,
            Delta::<f32>::default(),
        );

        assert_eq!(context_dimension.columns, 66);
        assert_eq!(context_dimension.lines, 88);

        let rich_text_id = 0;
        let route_id = 0;
        let context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            route_id,
            rich_text_id,
            context_dimension,
        );
        assert_eq!(update_title(String::from(""), &context), String::from(""));
        assert_eq!(
            update_title(String::from("{{columns}}"), &context),
            String::from("66")
        );
        assert_eq!(
            update_title(String::from("{{COLUMNS}}"), &context),
            String::from("66")
        );
        assert_eq!(
            update_title(String::from("{{ COLUMNS }}"), &context),
            String::from("66")
        );
        assert_eq!(
            update_title(String::from("{{ columns }}"), &context),
            String::from("66")
        );
        assert_eq!(
            update_title(String::from("hello {{ COLUMNS }} AbC"), &context),
            String::from("hello 66 AbC")
        );
    }
}
