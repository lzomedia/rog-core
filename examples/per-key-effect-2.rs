use daemon::aura::{BuiltInModeByte, Key, KeyColourArray};
use daemon::daemon::{DBUS_IFACE, DBUS_NAME, DBUS_PATH};
use dbus::Error as DbusError;
use dbus::{ffidisp::Connection, Message};
use std::{thread, time};

pub fn dbus_led_builtin_write(bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let bus = Connection::new_system()?;
    //let proxy = bus.with_proxy(DBUS_IFACE, "/", Duration::from_millis(5000));
    let msg = Message::new_method_call(DBUS_NAME, DBUS_PATH, DBUS_IFACE, "ledmessage")?
        .append1(bytes.to_vec());
    let r = bus.send_with_reply_and_block(msg, 5000)?;
    if let Some(reply) = r.get1::<&str>() {
        println!("Success: {:x?}", reply);
        return Ok(());
    }
    Err(Box::new(DbusError::new_custom("name", "message")))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bus = Connection::new_system()?;

    let mut per_key_led = Vec::new();
    let mut key_colours = KeyColourArray::new();
    per_key_led.push(key_colours.clone());

    for _ in 0..29 {
        *key_colours.key(Key::ROG).0 += 8;
        *key_colours.key(Key::L).0 += 8;
        *key_colours.key(Key::I).0 += 8;
        *key_colours.key(Key::N).0 += 8;
        *key_colours.key(Key::U).0 += 8;
        *key_colours.key(Key::X).0 += 8;
        per_key_led.push(key_colours.clone());
    }

    for _ in 0..29 {
        *key_colours.key(Key::ROG).0 -= 8;
        *key_colours.key(Key::L).0 -= 8;
        *key_colours.key(Key::I).0 -= 8;
        *key_colours.key(Key::N).0 -= 8;
        *key_colours.key(Key::U).0 -= 8;
        *key_colours.key(Key::X).0 -= 8;
        per_key_led.push(key_colours.clone());
    }

    // It takes each interrupt at least 1ms. 10ms to write complete block. Plus any extra
    // penalty time such as read waits
    let time = time::Duration::from_millis(16); // aim for 60 per second

    let row = KeyColourArray::get_init_msg();
    let msg =
        Message::new_method_call(DBUS_NAME, DBUS_PATH, DBUS_IFACE, "ledmessage")?.append1(row);
    bus.send(msg).unwrap();

    loop {
        let now = std::time::Instant::now();
        thread::sleep(time);

        for group in &per_key_led {
            thread::sleep(time);
            let group = group.get();
            let msg = Message::new_method_call(DBUS_NAME, DBUS_PATH, DBUS_IFACE, "ledeffect")?
                .append1(&group[0].to_vec())
                .append1(&group[1].to_vec())
                .append1(&group[2].to_vec())
                .append1(&group[3].to_vec())
                .append1(&group[4].to_vec())
                .append1(&group[5].to_vec())
                .append1(&group[6].to_vec())
                .append1(&group[7].to_vec())
                .append1(&group[8].to_vec())
                .append1(&group[9].to_vec());
            bus.send(msg).unwrap();
        }
        let after = std::time::Instant::now();
        let diff = after.duration_since(now);
        dbg!(diff.as_millis());
        //return Ok(());
    }
}
