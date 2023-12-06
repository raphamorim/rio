use rio_backend::event::RioEvent;
use rio_backend::superloop::Superloop;

struct CustomListerner {
    proxy: Superloop,
    id: usize,
}

impl CustomListerner {
    pub fn new(proxy: Proxy, id: usize) -> CustomListerner {
        CustomListerner { proxy, id }
    }

    pub fn custom_send(&mut self, event: RioEvent) {
        self.proxy.send_event(event, self.id);
    }
}

fn main() {
    let mut superloop: Superloop = Superloop::new();

    // std::thread::spawn(|| {
    let proxy_one = superloop.create_proxy();
    let mut listener_one = CustomListerner::new(proxy_one, 0);
    listener_one.custom_send(RioEvent::Wakeup);
    // });

    let proxy_two = superloop.create_proxy();
    let mut listener_two = CustomListerner::new(proxy_two, 1);
    listener_two.custom_send(RioEvent::Bell);

    let proxy_three = superloop.create_proxy();
    let mut listener_three = CustomListerner::new(proxy_three, 2);
    listener_three.custom_send(RioEvent::Render);

    loop {
        match superloop.event() {
            RioEvent::Render => {
                println!("has Render");
            }
            RioEvent::Bell => {
                println!("has Bell");
            }
            RioEvent::Wakeup => {
                println!("has Wakeup");
            }
            RioEvent::Noop => {
                println!("reached Noop");
                break;
            }
            _ => {}
        }
    }
}
