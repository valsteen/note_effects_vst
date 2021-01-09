#[cfg(target_os = "macos")]
#[inline]
pub fn second_to_mach_timebase() -> f64 {
    let mut timebase_info = mach::mach_time::mach_timebase_info { numer: 0, denom: 0 } ;
    unsafe { mach::mach_time::mach_timebase_info(&mut timebase_info) ;}

    10e9 * timebase_info.denom as f64 / timebase_info.numer as f64
}
