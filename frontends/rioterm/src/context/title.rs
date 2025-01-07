use crate::context::Context;

#[inline]
fn update_title<T: rio_backend::event::EventListener>(template: String, context: Context<T>) -> String {
	let columns = "{{COLUMNS}}";
	if template.contains(columns) {
		let _ = template.replace(columns, "66");
	}
	template
}

#[cfg(test)]
pub mod test {
	use super::*;
	use crate::context::ContextDimension;
	use crate::context::Delta;
    use crate::context::create_mock_context;
    use rio_backend::event::VoidListener;
	use rio_window::window::WindowId;
	use rio_backend::sugarloaf::layout::SugarDimensions;

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
    	assert_eq!(String::from(""), update_title(String::from(""), context));

    	let context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            route_id,
            rich_text_id,
            context_dimension,
        );
    	assert_eq!(String::from("66"), update_title(String::from("{{COLUMNS}}"), context));
    }
}