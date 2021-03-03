use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use anyhow::Result;
use clap::{Arg, App as ClapApp, SubCommand, AppSettings};
use std::io::prelude::*;
use nannou::prelude::*;
use once_cell::sync::OnceCell;
use nannou::image::{RgbImage, DynamicImage, Rgb};

const CMD_DEVICES : &str = "devices";
const CMD_RUN : &str = "run";
const ARG_DEVICEID : &str = "deviceid";

static VIDEO_FRAME: OnceCell<std::sync::RwLock<Vec<u8>>> = OnceCell::new();

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

    std::thread::spawn(move || {
        capture_video(device_id).expect("failed to capture video");
    });

    nannou::app(model).run();
}


struct Model {
}

fn model(app: &App) -> Model {
    // Create a new window!
    app.new_window().size(512, 512).view(view).build().unwrap();
    // Load the image from disk and upload it to a GPU texture.
    Model { }
}

// Draw the state of your `Model` into the given `Frame` here.
fn view(app: &App, model: &Model, frame: Frame) {
    frame.clear(BLACK);

    let draw = app.draw();

    if let Some(video_frame) = VIDEO_FRAME.get() {
        if let Ok(video_frame) = video_frame.try_read() {
            let mut global : u64 = 0;
            let mut image = RgbImage::new(640, 480);
            let mut offset = 0;
            for y in 0..480 {
                for x in 0..640 {
                    let treshold = 200;
                    let v = video_frame[offset];
                    if v > treshold {
                        global+=1;
                        image.put_pixel(x, y, Rgb([255,0,0]));
                    } else {
                        image.put_pixel(x, y, Rgb([v,v,v]));
                    }
                    offset += 2;
                }
            }

            let image = DynamicImage::ImageRgb8(image);
            let texture = wgpu::Texture::from_image(app, &image);
            draw.texture(&texture);

            draw.text(&format!("{}",global));
        }
    }

    draw.to_frame(app, &frame).unwrap();
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
        fps: 30,
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

    // Get a stream, calling the closure as callback for every frame
    let image : Vec<u8> = [0u8;614400].to_vec();
    let image_rw = std::sync::RwLock::new(image);
    VIDEO_FRAME.set(image_rw).expect("cannot set image_rw");
    let stream = streamh
        .start_stream(
            |_frame, count| {
                if let Some(video_frame) = VIDEO_FRAME.get() {
                    if let Ok(mut instance) = video_frame.write() {
                        instance.clear();
                        instance.append(&mut Vec::from(_frame.to_bytes()));
                    }
                }
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
    } else {
        unreachable!()
    }
}
