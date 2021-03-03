use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use anyhow::Result;
use clap::{Arg, App as ClapApp, SubCommand, AppSettings};
use std::io::prelude::*;
use once_cell::sync::OnceCell;
use sysfs_gpio::{Direction, Pin};
use std::thread::sleep;

const CMD_DEVICES : &str = "devices";
const CMD_RUN : &str = "run";
const CMD_LEDSTEST : &str = "ledstest";
const ARG_DEVICEID : &str = "deviceid";

struct GPIO{
    pins: Vec<Pin>
}
impl GPIO {
    pub fn new() -> Self {
        let mut pins = Vec::new();
        for pin_no in 11..=18 {
            pins.push(Pin::new(pin_no));
        }
        GPIO { pins }
    }
    pub fn init(&self) -> Result<()> {
        for pin in &self.pins {
            if !pin.is_exported() {
                pin.export().map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, format!("Cannot export pin {}: {}",pin.get_pin(), err) ))?;
            }
            if pin.get_direction()? != Direction::Out {
                pin.set_direction(Direction::Out)?;
            }
            if pin.get_value()? != 1 {
                pin.set_value(1)?;
            }
        }
        Ok(())
    }
    pub fn test(&self) -> Result<()> {
        for pin in &self.pins {
            println!("Activating PIN {}",pin.get_pin());
            pin.set_value(0)?;
            std::thread::sleep( std::time::Duration::from_millis(200) );
            pin.set_value(1)?;
        }
        Ok(())
    }

    pub fn signal(&self, level: usize) -> Result<()> {
        println!("SIGNAL: {}",level);
        let len = self.pins.len();
        for n in 0..len {
            let value = if n > level { 1 } else { 0 };
            self.pins[n].set_value(value)?;
            println!("P {} {}",n,value);
        }
        Ok(())
    }
}


fn cmd_device_list() -> Result<()> {
    let ctx = uvc::Context::new().expect("Could not get context");

    let devices = ctx.devices()?;

    let mut count = 0;
    for device in devices {
        let description = device.description()?;
        println!("- {}:{} {:?} {:?}",device.bus_number(),device.device_address(),
            description.product,description.serial_number);
        count += 1;
    }
    println!("Found {} devices",count);

    Ok(())
}

fn cmd_run(device_id : String) {
    capture_video(device_id).expect("failed to capture video");
}

fn cmd_leds_test() {
    let gpio = GPIO::new();
    gpio.init().expect("Cannot init GPIOs");
    gpio.test().expect("Cannot test GPIOs");
}

fn capture_video(device_id : String) -> Result<()> {
    // Get a libuvc context
    let ctx = uvc::Context::new()?;
    let bus_address :Vec<_> = device_id.split(":").collect();
    let bus = u8::from_str_radix(bus_address[0],10)?;
    let address = u8::from_str_radix(bus_address[1],10)?;

    let dev = ctx.devices()?.find(|d| bus==d.bus_number() && address==d.device_address());
    if dev.is_none() {
        println!("Device not found");
        return Ok(());
    }
    let dev = dev.unwrap();

    // The device must be opened to create a handle to the device
    let devh = {
        match dev.open() {
            Err(what) if what.to_string() == "Access denied" => {
                println!("Access error, run: sudo chmod 0666 /dev/bus/usb/{:03}/{:03}",bus,address);
                return Ok(());
            }
            Err(err) => return Err(err.into()),
            Ok(devh) => devh
        }
    };

    // Most webcams support this format
    let format = uvc::StreamFormat {
        width: 640,
        height: 480,
        fps: 10,
        format: uvc::FrameFormat::YUYV,
    };

    devh.set_ae_mode(uvc::AutoExposureMode::Manual).expect("cannot disable auto exposure");
    // Get the necessary stream information
    let mut streamh = devh
        .get_stream_handle_with_format(format)
        .expect("Could not open a stream with this format");

    // This is a counter, increasing by one for every frame
    // This data must be 'static + Send + Sync to be used in
    // the callback used in the stream
    let counter = Arc::new(AtomicUsize::new(0));

    let gpio = GPIO::new();
    gpio.init()?;

    // Get a stream, calling the closure as callback for every frame
    let image : Vec<u8> = [0u8;614400].to_vec();
    let image_rw = std::sync::RwLock::new(image);
    let stream = streamh
        .start_stream(
            move |_frame, count| {
                let treshold = 230;
                let pindiv = 100;

                let video_frame = _frame.to_bytes();
                let mut global = 0;
                let mut offset = 0;
                for _ in 0..480 {
                    for _ in 0..640 {
                        let v = video_frame[offset];
                        if v > treshold {
                            global+=1;
                        }
                        offset += 2;
                    }
                }

                gpio.signal(global/pindiv as usize).expect("must set gpios");

                println!("{}",global);
                count.fetch_add(1, Ordering::SeqCst);
            },
            counter.clone(),
        ).expect("Could not start stream");

    // Wait 10 seconds
    std::thread::sleep(Duration::new(180, 0));

    // Explicitly stop the stream
    // The stream would also be stopped
    // when going out of scope (dropped)
    stream.stop();
    println!("Counter: {}", counter.load(Ordering::SeqCst));

    return Ok(());

}

fn main() -> Result<()> {
    let matches = ClapApp::new("Nau3 maimai")
        .subcommand(
            SubCommand::with_name(CMD_DEVICES)
            .about("Lists found local devices"))
        .subcommand(
            SubCommand::with_name(CMD_LEDSTEST)
            .about("Test leds"))
        .subcommand(
            SubCommand::with_name(CMD_RUN)
            .about("Execute the application")
            .arg(Arg::with_name(ARG_DEVICEID).required(true).takes_value(true).index(1)))
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .get_matches();

    if let Some(_) = matches.subcommand_matches(CMD_DEVICES) {
        cmd_device_list()
    } else if let Some(matches) = matches.subcommand_matches(CMD_RUN) {
        let device_id = matches.value_of(ARG_DEVICEID).unwrap();
        Ok(cmd_run(device_id.to_string()))
    } else if let Some(matches) = matches.subcommand_matches(CMD_LEDSTEST) {
        cmd_leds_test();
        Ok(())
    } else {
        unreachable!()
    }
}
