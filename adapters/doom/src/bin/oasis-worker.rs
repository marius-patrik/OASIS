//! Isolated GPL-scoped execution of source-native DOOM routines.
//! Full original DOOM gameplay globals and thinker loop remain integration work.
use oasis_adapter_doom::{DoomMathAdapter,MODULE_ID};
use oasis_contracts::{GameAdapter,Id};
use oasis_native_process::serve_native_boxed;

fn main(){
    let args=std::env::args().collect::<Vec<_>>();
    assert_eq!(args.len(),2,"usage: oasis-worker <context-u128>");
    let context=Id(args[1].parse().expect("invalid worker context"));
    let native=DoomMathAdapter.start_context(MODULE_ID,context)
        .expect("start pinned original DOOM module");
    serve_native_boxed(native).expect("DOOM worker terminated unexpectedly");
}
