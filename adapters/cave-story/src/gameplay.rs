//! Original Cave Story player and tile physics in a non-Send native worker.
//!
//! The source-pinned doukutsu-rs oasis_bridge owns the actual Player,
//! SharedGameState, NPCList, input controller and Stage. OASIS never
//! substitutes its own movement or collision resolution.
//!
//! IMPORTANT: The currently supported snapshot is a movement checkpoint,
//! NOT the full original scene/player state. Replaying complex source-game
//! events/boosters, scripts or NPC interactions requires a larger source-owned
//! snapshot implementation before production portability.
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

use doukutsu_rs::oasis_bridge::{Buttons, PlayerFrame, Simulation};
use oasis_contracts::{
    AdapterRegistry, ClockStep, ContractError, ContractResult, DefinitionRef,
    EntityView, GameAdapter, Id, InputIntent, ModuleDescriptor, NativeHandle,
    NativeModule, Revision, Snapshot, StepOutput, TypeRef, TypedValue, Value,
    WorldPort,
};
use oasis_native_process::{LocalNativeModule, ProcessModule};

use crate::ENGINE_ID;

pub const MODULE_ID: Id = Id(0xc451_0032);
pub const DEFINITION_ID: Id = Id(0xc451_0033);
const STAGE: &str = "player.stage-pxm";
const STATE: &str = "player.movement-state";
const CONTROL: &str = "player.controls";
const FRAME: &str = "player.frame";

pub fn kind(name:&str)->TypeRef{
    TypeRef{namespace:"cave-story".into(),name:name.into(),version:1}
}
fn invalid(reason:&str)->ContractError{ContractError::InvalidData(reason.into())}
fn typed_map(v:&Value)->ContractResult<&BTreeMap<String,Value>>{
    match v {Value::Map(m)=>Ok(m),_=>Err(invalid("native component must be map"))}
}
fn find<'a>(state:&'a [TypedValue],name:&str)->ContractResult<&'a Value>{
    state.iter().find(|v|v.type_ref==kind(name))
        .map(|v|&v.data).ok_or_else(||invalid("missing originating native component"))
}
fn read_i32(map:&BTreeMap<String,Value>,name:&str)->ContractResult<i32>{
    match map.get(name){
        Some(Value::Int(v))=>i32::try_from(*v).map_err(|_|invalid("coordinate overflow")),
        _=>Err(invalid("original fixed-point coordinate expected")),
    }
}
fn read_u32(map:&BTreeMap<String,Value>,name:&str)->ContractResult<u32>{
    match map.get(name){
        Some(Value::UInt(v))=>u32::try_from(*v).map_err(|_|invalid("original flag overflow")),
        _=>Err(invalid("original unsigned flag expected")),
    }
}
fn read_u64(map:&BTreeMap<String,Value>,name:&str)->ContractResult<u64>{
    match map.get(name){
        Some(Value::UInt(v))=>Ok(*v),
        _=>Err(invalid("original tick expected")),
    }
}
fn as_bytes<'a>(map:&'a BTreeMap<String,Value>,name:&str)->ContractResult<&'a [u8]>{
    match map.get(name){
        Some(Value::Bytes(v))=>Ok(v),
        _=>Err(invalid("original PXM and tile attributes must be raw bytes")),
    }
}
fn state_frame(snapshot:&Snapshot)->ContractResult<(PlayerFrame,u64)>{
    let map=typed_map(find(&snapshot.state,STATE)?)?;
    let life=read_u32(map,"life")?;
    let frame=PlayerFrame{
        x:read_i32(map,"x")?,
        y:read_i32(map,"y")?,
        vel_x:read_i32(map,"vel_x")?,
        vel_y:read_i32(map,"vel_y")?,
        life:u16::try_from(life).map_err(|_|invalid("original life overflow"))?,
        collision_flags:read_u32(map,"collision_flags")?,
    };
    Ok((frame,read_u64(map,"native_tick")?))
}
fn update_frame(snapshot:&mut Snapshot,frame:PlayerFrame,tick:u64)->ContractResult<()>{
    let state=snapshot.state.iter_mut().find(|v|v.type_ref==kind(STATE))
        .ok_or_else(||invalid("missing original movement-state component"))?;
    let Value::Map(map)=&mut state.data else{
        return Err(invalid("original player state malformed"));
    };
    for (name,val) in [
        ("x",Value::Int(i64::from(frame.x))),
        ("y",Value::Int(i64::from(frame.y))),
        ("vel_x",Value::Int(i64::from(frame.vel_x))),
        ("vel_y",Value::Int(i64::from(frame.vel_y))),
        ("life",Value::UInt(u64::from(frame.life))),
        ("collision_flags",Value::UInt(u64::from(frame.collision_flags))),
        ("native_tick",Value::UInt(tick)),
    ] {
        map.insert(name.into(),val);
    }
    Ok(())
}
fn fresh_native(snapshot:&Snapshot)->ContractResult<(Simulation,u64)>{
    let stage=typed_map(find(&snapshot.state,STAGE)?)?;
    let mut engine=Simulation::new().map_err(|e|invalid(&format!("original game init: {e:?}")))?;
    engine.load_stage(as_bytes(stage,"pxm")?,as_bytes(stage,"attributes")?)
        .map_err(|e|invalid(&format!("original PXM stage: {e:?}")))?;
    let (frame,tick)=state_frame(snapshot)?;
    engine.restore_frame(frame);
    Ok((engine,tick))
}
fn buttons(v:&Value)->ContractResult<Buttons>{
    let map=typed_map(v)?;
    fn flag(map:&BTreeMap<String,Value>,name:&str)->ContractResult<bool>{
        match map.get(name){
            Some(Value::Bool(b))=>Ok(*b),
            None=>Ok(false),
            _=>Err(invalid("game input buttons must be boolean")),
        }
    }
    if map.keys().any(|key|!["left","right","up","down","jump","shoot"]
        .contains(&key.as_str())){
        return Err(invalid("unsupported original player input button"));
    }
    Ok(Buttons{
        left:flag(map,"left")?,right:flag(map,"right")?,
        up:flag(map,"up")?,down:flag(map,"down")?,
        jump:flag(map,"jump")?,shoot:flag(map,"shoot")?,
    })
}
struct PlayerInstance {
    entity: Id,
    sim: Simulation,
    snapshot: Snapshot,
    last_tick: u64,
    last_host_tick: Option<u64>,
}

/// Runs only inside its own game worker process. The source engine's
/// original !Send scene/context objects never cross the Rust host boundary.
pub struct OriginalPlayer {
    context: Id,
    instance: Option<PlayerInstance>,
    poisoned: bool,
}
impl OriginalPlayer {
    pub fn new(context:Id)->Self{
        Self{context,instance:None,poisoned:false}
    }
    fn instance(&self,handle:NativeHandle)->ContractResult<&PlayerInstance>{
        if handle.context_id!=self.context || handle.native_slot!=1{
            return Err(ContractError::StaleAuthority);
        }
        self.instance.as_ref().ok_or(ContractError::NotFound(handle.context_id))
    }
}
impl LocalNativeModule for OriginalPlayer {
    fn descriptor(&self)->ModuleDescriptor{
        ModuleDescriptor{
            engine_id:ENGINE_ID,module_id:MODULE_ID,
            contract_major:0,contract_minor:1,
            exported_interfaces:vec![kind(CONTROL),kind(FRAME)],
            required_interfaces:vec![],
        }
    }
    fn instantiate(&mut self,entity:&EntityView,saved:Option<&Snapshot>)
        ->ContractResult<NativeHandle>{
        if self.poisoned{return Err(ContractError::StaleAuthority);}
        if self.instance.is_some(){return Err(invalid("one source player per isolated context"));}
        if entity.origin_module!=Some(MODULE_ID){
            return Err(invalid("foreign native player module"));
        }
        let snapshot=saved.cloned().unwrap_or_else(||Snapshot{
            entity_id:entity.id,revision:entity.revision,
            state:entity.components.clone(),binary_artifact:None,
        });
        if snapshot.entity_id!=entity.id{
            return Err(invalid("foreign native player checkpoint"));
        }
        let (sim,last_tick)=fresh_native(&snapshot)?;
        self.instance=Some(PlayerInstance{entity:entity.id,sim,snapshot,last_tick,last_host_tick:None});
        Ok(NativeHandle{context_id:self.context,native_slot:1})
    }
    fn step(&mut self,clock:ClockStep,inputs:&[InputIntent],_world:&mut dyn WorldPort)
        ->ContractResult<StepOutput>{
        if self.poisoned{return Err(ContractError::StaleAuthority);}
        let instance=self.instance.as_mut().ok_or(ContractError::NotFound(self.context))?;
        if instance.last_host_tick.is_some_and(|last|clock.native_tick<=last){
            return Err(invalid("host world tick must advance"));
        }
        // Native character ticks are intrinsic to the original game, not
        // to whichever OASIS world shard currently hosts the character.
        // The destination shard may have its own clock starting at zero.
        let next_native_tick=instance.last_tick.checked_add(1)
            .ok_or_else(||invalid("origin-native player clock overflow"))?;
        if inputs.len()>1 {return Err(invalid("only one native control state per tick"));}
        let control=if let Some(input)=inputs.first(){
            if input.controller_entity_id!=instance.entity ||
               input.intent.type_ref!=kind(CONTROL) {
                return Err(invalid("unauthorized or unsupported player input"));
            }
            buttons(&input.intent.data)?
        } else {Buttons::default()};
        let next_revision=instance.snapshot.revision.0.checked_add(1)
            .ok_or_else(||invalid("native player revision overflow"))?;
        instance.sim.controls(control);
        // Once native simulation begins, any failure quarantines the child
        // rather than allowing a nontransactional partial tick to continue.
        let frame=match instance.sim.tick(){
            Ok(frame)=>frame,
            Err(error)=>{
                self.poisoned=true;
                return Err(ContractError::Internal(
                    format!("original source game tick failed: {error:?}")
                ));
            },
        };
        // All mandatory checkpoint fields were validated at instantiation.
        update_frame(&mut instance.snapshot,frame,next_native_tick)?;
        instance.snapshot.revision=Revision(next_revision);
        instance.last_tick=next_native_tick;
        instance.last_host_tick=Some(clock.native_tick);
        Ok(StepOutput{
            state_changes:vec![instance.snapshot.clone()],
            interactions:vec![],
            emitted_events:vec![TypedValue{
                type_ref:kind(FRAME),
                data:find(&instance.snapshot.state,STATE)?.clone(),
            }],
        })
    }
    fn snapshot(&self,handle:NativeHandle)->ContractResult<Snapshot>{
        if self.poisoned{return Err(ContractError::StaleAuthority);}
        Ok(self.instance(handle)?.snapshot.clone())
    }
    fn restore(&mut self,handle:NativeHandle,snapshot:&Snapshot)->ContractResult<()>{
        if self.poisoned{return Err(ContractError::StaleAuthority);}
        if self.instance(handle)?.entity!=snapshot.entity_id{
            return Err(invalid("cannot restore foreign native player"));
        }
        let (sim,last_tick)=fresh_native(snapshot)?;
        self.instance=Some(PlayerInstance{
            entity:snapshot.entity_id,sim,snapshot:snapshot.clone(),last_tick,
            last_host_tick:None,
        });
        Ok(())
    }
    fn remove(&mut self,handle:NativeHandle)->ContractResult<()>{
        self.instance(handle)?;
        self.instance=None;
        Ok(())
    }
}

/// OASIS discovers this source-native module through the SAME generic
/// catalog. It creates a process-backed Send NativeModule that owns a
/// separately instantiated non-Send original Cave Story simulation.
pub struct OriginalPlayerAdapter {
    pub worker_path: PathBuf,
}
impl GameAdapter for OriginalPlayerAdapter {
    fn register(&self,registry:&mut dyn AdapterRegistry)->ContractResult<()>{
        registry.register_module(ModuleDescriptor{
            engine_id:ENGINE_ID,module_id:MODULE_ID,
            contract_major:0,contract_minor:1,
            exported_interfaces:vec![kind(CONTROL),kind(FRAME)],
            required_interfaces:vec![],
        })?;
        registry.register_definition(DefinitionRef{id:DEFINITION_ID,version:1})?;
        registry.register_capability(kind(CONTROL),MODULE_ID)
    }
    fn start_context(&self,module_id:Id,context_id:Id)
        ->ContractResult<Box<dyn NativeModule>>{
        if module_id!=MODULE_ID {return Err(ContractError::NotFound(module_id));}
        let mut command=Command::new(&self.worker_path);
        command.arg("player").arg(context_id.0.to_string());
        Ok(Box::new(ProcessModule::spawn(&mut command,module_id,context_id)?))
    }
}
