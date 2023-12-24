use wa::native::apple::menu::RepresentedItem;
use wa::KeyAssignment;

mod macos;

pub fn create_menu(app_connection: corcovado::channel::Receiver<RepresentedItem>) {
    #[cfg(target_os = "macos")]
    {
        macos::create_menu();

        // tokio::spawn(async move {
        //     let poll = corcovado::Poll::new().unwrap();
        //     loop {
        //         while let Ok(request) = app_connection.try_recv() {
        //             match request {
        //                 RepresentedItem::KeyAssignment(KeyAssignment::SpawnWindow) => {
        //                     println!("SpawnWindow");
        //                 },
        //                 RepresentedItem::KeyAssignment(KeyAssignment::Copy(text)) => {
        //                     println!("Copy {text}");
        //                 },
        //             }
        //         }
        //     }
        // });
    }
}
