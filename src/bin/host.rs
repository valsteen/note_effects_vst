extern crate vst;

use std::env;
use std::path::Path;
use std::sync::{Arc, Mutex};

use vst::host::{Host, PluginLoader, HostBuffer};
use vst::plugin::Plugin;
use vst::plugin::CanDo;
use vst::api::Supported;

#[allow(dead_code)]
struct SampleHost;

impl Host for SampleHost {
    fn automate(&self, index: i32, value: f32) {
        println!("Parameter {} had its value changed to {}", index, value);
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let arg = if args.len() < 2 {
        "target/debug/libnote_generator.dylib"
    } else {
        &args[1]
    };
    let path = Path::new(arg);

    // Create the host
    let host = Arc::new(Mutex::new(SampleHost));

    println!("Loading {}...", path.to_str().unwrap());

    // Load the plugin
    let mut loader =
        PluginLoader::load(path, Arc::clone(&host)).unwrap_or_else(|e| panic!("Failed to load plugin: {}", e));

    // Create an instance of the plugin
    let mut instance = loader.instance().unwrap();

    // Get the plugin information
    let info = instance.get_info();

    println!(
        "Loaded '{}':\n\t\
         Vendor: {}\n\t\
         Presets: {}\n\t\
         Parameters: {}\n\t\
         VST ID: {}\n\t\
         Version: {}\n\t\
         Initial Delay: {} samples",
        info.name, info.vendor, info.presets, info.parameters, info.unique_id, info.version, info.initial_delay
    );

    // Initialize the instance
    instance.init();

    println!("{}", instance.can_do(CanDo::Offline) == Supported::No);
    println!("{}", instance.can_do(CanDo::ReceiveEvents) == Supported::Yes);
    let mut host_buffer : HostBuffer<f32> = HostBuffer::new(2,2);
    let inputs = vec![vec![0.0; 1000]; 2];
    let mut outputs = vec![vec![0.0; 1000]; 2];
    let mut audio_buffer = host_buffer.bind(&inputs, &mut outputs);
    instance.process(&mut audio_buffer);
    let parameters = instance.get_parameter_object();
    parameters.set_parameter(5, 0.0);
    instance.process(&mut audio_buffer);
    parameters.set_parameter(5, 0.4);
    instance.process(&mut audio_buffer);
    parameters.set_parameter(5, 0.6);
    instance.process(&mut audio_buffer);
    parameters.set_parameter(5, 0.7);
    instance.process(&mut audio_buffer);
    parameters.set_parameter(5, 0.4);
    instance.process(&mut audio_buffer);
    parameters.set_parameter(5, 0.2);
    instance.process(&mut audio_buffer);
    println!("Initialized instance!");

    println!("Closing instance...");
    // Close the instance. This is not necessary as the instance is shut down when
    // it is dropped as it goes out of scope.
    // drop(instance);
}
