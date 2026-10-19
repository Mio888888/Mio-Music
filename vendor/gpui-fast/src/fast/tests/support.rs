// Modified by Longbridge for gpui-fast.
//! Test contexts the tests of gpui-fast's additions need beyond upstream's.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use crate::{
    App, BackgroundExecutor, ForegroundExecutor, Platform as _, PlatformTextSystem, TestAppContext,
    TestDispatcher, TestPlatform, TextSystem, app::GpuiMode,
};

impl TestAppContext {
    /// Creates a `TestAppContext` whose platform lays out and rasterizes text
    /// with the given text system rather than the no-op one.
    pub(crate) fn with_text_system(text_system: Arc<dyn PlatformTextSystem>) -> Self {
        let dispatcher = TestDispatcher::new(0);
        let arc_dispatcher = Arc::new(dispatcher.clone());
        let background_executor = BackgroundExecutor::new(arc_dispatcher.clone());
        let foreground_executor = ForegroundExecutor::new(arc_dispatcher);
        let platform = TestPlatform::with_text_system(
            background_executor.clone(),
            foreground_executor.clone(),
            text_system,
        );
        let asset_source = Arc::new(());
        let http_client = http_client::FakeHttpClient::with_404_response();
        let text_system = Arc::new(TextSystem::new(platform.text_system()));

        let app = App::new_app(platform.clone(), asset_source, http_client);
        app.borrow_mut().mode = GpuiMode::test();

        Self {
            app,
            background_executor,
            foreground_executor,
            dispatcher,
            test_platform: platform,
            text_system,
            fn_name: None,
            on_quit: Rc::new(RefCell::new(Vec::default())),
        }
    }
}
