//! Isolated execution of actual pinned Cave Story Rust modules.
//! The original logic remains inside doukutsu-rs. This is not a full game loop.
use oasis_adapter_cave_story::{
    CaveStoryAdapter,stage::CaveStoryStageAdapter,weapon::CaveStoryWeaponAdapter,
};
use oasis_contracts::{GameAdapter,Id};
use oasis_native_process::serve_native_boxed;

fn main(){
    let args=std::env::args().collect::<Vec<_>>();
    assert_eq!(args.len(),3,"usage: oasis-worker <hitbox|weapon|stage> <context-u128>");
    let context=Id(args[2].parse().expect("invalid worker context"));
    let (adapter,module): (Box<dyn GameAdapter>,Id)=match args[1].as_str(){
        "hitbox"=>(Box::new(CaveStoryAdapter),oasis_adapter_cave_story::MODULE_ID),
        "weapon"=>(Box::new(CaveStoryWeaponAdapter),
            oasis_adapter_cave_story::weapon::MODULE_ID),
        "stage"=>(Box::new(CaveStoryStageAdapter),
            oasis_adapter_cave_story::stage::MODULE_ID),
        _=>panic!("unknown native Cave Story module"),
    };
    let native=adapter.start_context(module,context).expect("start native engine context");
    serve_native_boxed(native).expect("native game worker terminated unexpectedly");
}
