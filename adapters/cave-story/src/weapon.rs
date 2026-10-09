//! Actual Cave Story ammunition behavior from the pinned upstream Weapon.
//!
//! This module uses `doukutsu_rs::game::weapon::Weapon` directly and delegates
//! consume/refill to the original implementation. It does NOT approximate
//! native projectiles, fire rate, inventory rules, or full gameplay ticking.
//! All persisted state is arbitrary, origin-typed OASIS data.

use std::collections::BTreeMap;

use doukutsu_rs::game::weapon::{Weapon, WeaponLevel, WeaponType};
use oasis_contracts::{
    AdapterRegistry, ClockStep, ContractError, ContractResult, DefinitionRef,
    EntityView, GameAdapter, Id, InputIntent, ModuleDescriptor, NativeHandle,
    NativeModule, Snapshot, StepOutput, TypeRef, TypedValue, Value, WorldPort,
};

use crate::ENGINE_ID;

pub const MODULE_ID: Id = Id(0xc451_0012);
pub const DEFINITION_ID: Id = Id(0xc451_0013);

fn kind(name: &str) -> TypeRef {
    TypeRef { namespace: "cave-story".into(), name: name.into(), version: 1 }
}
const STATE: &str = "weapon-native";
const CONSUME: &str = "weapon.consume-ammo";
const REFILL: &str = "weapon.refill-ammo";
const RESULT: &str = "weapon.ammo-result";

fn invalid(message: &str) -> ContractError {
    ContractError::InvalidData(message.into())
}
fn native_map(value: &Value) -> ContractResult<&BTreeMap<String, Value>> {
    match value {
        Value::Map(map) => Ok(map),
        _ => Err(invalid("source-native weapon state must be a map")),
    }
}
fn field_u16(map: &BTreeMap<String, Value>, name: &str) -> ContractResult<u16> {
    let Some(Value::UInt(value))=map.get(name) else {
        return Err(invalid("source-native weapon field must be an unsigned integer"));
    };
    u16::try_from(*value).map_err(|_|invalid("source-native weapon field exceeds u16"))
}
fn native_type(id: u16) -> ContractResult<WeaponType> {
    // The mapping is to the SOURCE game enum, not to another game's weapons.
    Ok(match id {
        0 => WeaponType::None,
        1 => WeaponType::Snake,
        2 => WeaponType::PolarStar,
        3 => WeaponType::Fireball,
        4 => WeaponType::MachineGun,
        5 => WeaponType::MissileLauncher,
        7 => WeaponType::Bubbler,
        9 => WeaponType::Blade,
        10 => WeaponType::SuperMissileLauncher,
        12 => WeaponType::Nemesis,
        13 => WeaponType::Spur,
        _ => return Err(invalid("unsupported upstream Cave Story weapon ID")),
    })
}
fn native_level(id: u16) -> ContractResult<WeaponLevel> {
    Ok(match id {
        0 => WeaponLevel::None,
        1 => WeaponLevel::Level1,
        2 => WeaponLevel::Level2,
        3 => WeaponLevel::Level3,
        _ => return Err(invalid("unsupported upstream Cave Story weapon level")),
    })
}
fn from_snapshot(snapshot: &Snapshot) -> ContractResult<Weapon> {
    let state = snapshot.state.iter().find(|part|part.type_ref==kind(STATE))
        .ok_or_else(||invalid("origin-native weapon component missing"))?;
    let map=native_map(&state.data)?;
    Ok(Weapon::new(
        native_type(field_u16(map,"weapon_type")?)?,
        native_level(field_u16(map,"level")?)?,
        field_u16(map,"experience")?,
        field_u16(map,"ammo")?,
        field_u16(map,"max_ammo")?,
    ))
}
fn save_ammo(snapshot: &mut Snapshot, native: &Weapon) -> ContractResult<()> {
    let part=snapshot.state.iter_mut().find(|part|part.type_ref==kind(STATE))
        .ok_or_else(||invalid("native weapon component missing"))?;
    let Value::Map(map)=&mut part.data else {
        return Err(invalid("native weapon component is not a map"));
    };
    map.insert("ammo".into(),Value::UInt(u64::from(native.ammo)));
    Ok(())
}

struct NativeInstance {
    id: Id,
    weapon: Weapon,
    snapshot: Snapshot,
}

pub struct CaveStoryWeaponAdapter;

impl GameAdapter for CaveStoryWeaponAdapter {
    fn register(&self, registry: &mut dyn AdapterRegistry) -> ContractResult<()> {
        registry.register_module(ModuleDescriptor {
            engine_id: ENGINE_ID, module_id: MODULE_ID,
            contract_major:0,contract_minor:1,
            exported_interfaces:vec![kind(CONSUME),kind(REFILL)],
            required_interfaces:vec![],
        })?;
        registry.register_definition(DefinitionRef{id:DEFINITION_ID,version:1})?;
        registry.register_capability(kind(CONSUME),MODULE_ID)?;
        registry.register_capability(kind(REFILL),MODULE_ID)
    }

    fn start_context(&self, module_id: Id, context_id: Id)
        -> ContractResult<Box<dyn NativeModule>> {
        if module_id!=MODULE_ID {return Err(ContractError::NotFound(module_id));}
        Ok(Box::new(CaveStoryWeaponModule {
            context_id,next_slot:0,last_tick:0,instances:BTreeMap::new(),
        }))
    }
}

pub struct CaveStoryWeaponModule {
    context_id: Id,
    next_slot: u64,
    last_tick: u64,
    instances: BTreeMap<u64,NativeInstance>,
}

impl CaveStoryWeaponModule {
    fn instance(&self,handle:NativeHandle) -> ContractResult<&NativeInstance> {
        if handle.context_id!=self.context_id {
            return Err(ContractError::StaleAuthority);
        }
        self.instances.get(&handle.native_slot)
            .ok_or(ContractError::NotFound(self.context_id))
    }
}

impl NativeModule for CaveStoryWeaponModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            engine_id:ENGINE_ID,module_id:MODULE_ID,
            contract_major:0,contract_minor:1,
            exported_interfaces:vec![kind(CONSUME),kind(REFILL)],
            required_interfaces:vec![],
        }
    }
    fn instantiate(&mut self,entity:&EntityView,saved:Option<&Snapshot>)
        -> ContractResult<NativeHandle> {
        if entity.origin_module!=Some(MODULE_ID) {
            return Err(invalid("weapon entity has wrong originating module"));
        }
        let snapshot=saved.cloned().unwrap_or_else(||Snapshot {
            entity_id:entity.id,state:entity.components.clone(),
            revision:entity.revision,binary_artifact:None,
        });
        if snapshot.entity_id!=entity.id {
            return Err(invalid("native weapon checkpoint belongs to another item"));
        }
        let weapon=from_snapshot(&snapshot)?;
        if self.instances.values().any(|instance|instance.id==entity.id) {
            return Err(invalid("native weapon already instantiated"));
        }
        let next=self.next_slot.checked_add(1)
            .ok_or_else(||invalid("native weapon handles exhausted"))?;
        self.next_slot=next;
        self.instances.insert(next,NativeInstance{id:entity.id,weapon,snapshot});
        Ok(NativeHandle{context_id:self.context_id,native_slot:next})
    }
    fn step(&mut self,clock:ClockStep,inputs:&[InputIntent],_world:&mut dyn WorldPort)
        -> ContractResult<StepOutput> {
        if clock.native_tick<=self.last_tick {
            return Err(invalid("native weapon clock must advance"));
        }
        // Each item runs its originating weapon logic, irrespective of the
        // current world. OASIS does not implement its consume/refill formulas.
        let mut changed=BTreeMap::new();
        let mut events=Vec::new();
        for input in inputs {
            let op=&input.intent.type_ref;
            if *op!=kind(CONSUME) && *op!=kind(REFILL) {
                return Err(ContractError::Unsupported {
                    interface:op.clone(),reason:"unknown native weapon action".into()
                });
            }
            let amount=match &input.intent.data {
                Value::UInt(amount) => u16::try_from(*amount)
                    .map_err(|_|invalid("native ammo amount exceeds u16"))?,
                _ => return Err(invalid("native ammo amount must be unsigned")),
            };
            let instance=self.instances.values_mut()
                .find(|instance|instance.id==input.controller_entity_id)
                .ok_or(ContractError::NotFound(input.controller_entity_id))?;
            let before=instance.weapon.ammo;
            let accepted=if *op==kind(CONSUME) {
                instance.weapon.consume_ammo(amount)
            } else {
                instance.weapon.refill_ammo(amount);
                true
            };
            if instance.weapon.ammo!=before {
                save_ammo(&mut instance.snapshot,&instance.weapon)?;
                instance.snapshot.revision.0=instance.snapshot.revision.0.checked_add(1)
                    .ok_or_else(||invalid("native weapon revision overflow"))?;
                changed.insert(instance.id,instance.snapshot.clone());
            }
            events.push(TypedValue {
                type_ref:kind(RESULT),
                data:Value::Map(BTreeMap::from([
                    ("item".into(),Value::Ref(instance.id)),
                    ("accepted".into(),Value::Bool(accepted)),
                    ("ammo".into(),Value::UInt(u64::from(instance.weapon.ammo))),
                    ("max_ammo".into(),Value::UInt(u64::from(instance.weapon.max_ammo))),
                ])),
            });
        }
        self.last_tick=clock.native_tick;
        Ok(StepOutput {
            state_changes:changed.into_values().collect(),
            interactions:vec![],emitted_events:events,
        })
    }
    fn snapshot(&self,handle:NativeHandle)->ContractResult<Snapshot>{
        Ok(self.instance(handle)?.snapshot.clone())
    }
    fn restore(&mut self,handle:NativeHandle,saved:&Snapshot)->ContractResult<()>{
        let item=self.instance(handle)?.id;
        if saved.entity_id!=item {return Err(invalid("foreign native item checkpoint"));}
        let weapon=from_snapshot(saved)?;
        let instance=self.instances.get_mut(&handle.native_slot)
            .ok_or(ContractError::NotFound(self.context_id))?;
        instance.weapon=weapon;
        instance.snapshot=saved.clone();
        Ok(())
    }
    fn remove(&mut self,handle:NativeHandle)->ContractResult<()>{
        self.instance(handle)?;
        self.instances.remove(&handle.native_slot);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis_runtime::{Host,catalog::Catalog};

    fn weapon(ammo:u64,max_ammo:u64)->EntityView{
        EntityView{
            id:Id(0xabc),definition:DefinitionRef{id:DEFINITION_ID,version:1},
            revision:oasis_contracts::Revision(0),origin_module:Some(MODULE_ID),
            components:vec![
                TypedValue{
                    type_ref:kind(STATE),
                    data:Value::Map(BTreeMap::from([
                        ("weapon_type".into(),Value::UInt(2)), // original Polar Star
                        ("level".into(),Value::UInt(2)),
                        ("experience".into(),Value::UInt(9)),
                        ("ammo".into(),Value::UInt(ammo)),
                        ("max_ammo".into(),Value::UInt(max_ammo)),
                    ])),
                },
                TypedValue {
                    type_ref:TypeRef{namespace:"unrelated".into(),
                        name:"arbitrary".into(),version:99},
                    data:Value::Bytes(vec![1,2,254,255]),
                },
            ],
        }
    }
    fn action(name:&str,amount:u64)->InputIntent{
        InputIntent{
            controller_entity_id:Id(0xabc),
            intent:TypedValue{
                type_ref:kind(name),data:Value::UInt(amount),
            },
        }
    }
    fn ammo(snapshot:&Snapshot)->u64{
        let map=native_map(&snapshot.state[0].data).unwrap();
        match map.get("ammo") {Some(Value::UInt(v))=>*v,_=>panic!("invalid state")}
    }
    fn accepted(event:&TypedValue)->bool{
        let map=native_map(&event.data).unwrap();
        matches!(map.get("accepted"),Some(Value::Bool(true)))
    }
    fn clock(tick:u64)->ClockStep{
        ClockStep{native_tick:tick,delta_nanos:20_000_000,
            simulation_time_nanos:u128::from(tick)*20_000_000}
    }

    #[test]
    fn original_cave_story_weapon_ammunition_methods_survive_world_handoff(){
        let adapter=CaveStoryWeaponAdapter;
        let mut catalog=Catalog::new();
        catalog.install(&crate::CaveStoryAdapter).unwrap();
        catalog.install(&adapter).unwrap();
        assert_eq!(catalog.capability_provider(&kind(CONSUME)),Some(MODULE_ID));
        assert_eq!(catalog.capability_provider(&kind(REFILL)),Some(MODULE_ID));

        let entity=weapon(5,8);
        let mut first=Host::new();
        first.register_module(Id(100),adapter.start_context(MODULE_ID,Id(100)).unwrap()).unwrap();
        first.instantiate(Id(100),entity.clone(),None).unwrap();

        let output=first.step(Id(100),clock(1),&[action(CONSUME,3)]).unwrap();
        assert!(accepted(&output.emitted_events[0]));
        assert_eq!(output.state_changes.len(),1);
        assert_eq!(ammo(&first.snapshot(entity.id).unwrap()),2);
        assert_eq!(first.snapshot(entity.id).unwrap().revision.0,1);

        // Source game's actual ammo method refuses insufficient ammunition.
        let output=first.step(Id(100),clock(2),&[action(CONSUME,10)]).unwrap();
        assert!(!accepted(&output.emitted_events[0]));
        assert!(output.state_changes.is_empty());
        assert_eq!(ammo(&first.snapshot(entity.id).unwrap()),2);

        // The same global item can be instantiated in another host, using
        // precisely the originating source item logic, no partner-game code.
        let saved=first.snapshot(entity.id).unwrap();
        first.remove(entity.id).unwrap();
        let mut second=Host::new();
        second.register_module(Id(200),adapter.start_context(MODULE_ID,Id(200)).unwrap()).unwrap();
        second.instantiate(Id(200),entity.clone(),Some(&saved)).unwrap();
        let output=second.step(Id(200),clock(1),&[action(REFILL,20)]).unwrap();
        assert!(accepted(&output.emitted_events[0]));
        let loaded=second.snapshot(entity.id).unwrap();
        assert_eq!(ammo(&loaded),8);
        assert_eq!(loaded.revision.0,2);
        assert_eq!(loaded.state[1],saved.state[1]); // arbitrary native metadata
    }

    #[test]
    fn upstream_unlimited_ammo_and_invalid_weapon_ids_are_not_rewritten(){
        let adapter=CaveStoryWeaponAdapter;
        let mut module=adapter.start_context(MODULE_ID,Id(101)).unwrap();
        let entity=weapon(0,0); // Cave Story max_ammo=0 means unlimited
        let handle=module.instantiate(&entity,None).unwrap();

        struct UnusedWorld;
        impl WorldPort for UnusedWorld {
            fn geometry(&mut self,_:oasis_contracts::GeometryRequest)
                ->ContractResult<oasis_contracts::GeometryResult>{Err(invalid("unused"))}
            fn query(&mut self,_:oasis_contracts::SpatialQuery)
                ->ContractResult<Vec<oasis_contracts::SpatialHit>>{Err(invalid("unused"))}
            fn frame_map(&self,_:Id,_:Id)->ContractResult<oasis_contracts::FrameMap>
                {Err(invalid("unused"))}
            fn entity_view(&self,_:Id)->ContractResult<EntityView>{Err(invalid("unused"))}
            fn submit_interaction(&mut self,_:oasis_contracts::InteractionRequest)
                ->ContractResult<()>{Err(invalid("unused"))}
        }
        let out=module.step(clock(1),&[action(CONSUME,5)],&mut UnusedWorld).unwrap();
        assert!(accepted(&out.emitted_events[0]));
        assert_eq!(ammo(&module.snapshot(handle).unwrap()),0);
        assert_eq!(module.snapshot(handle).unwrap().revision.0,0);

        let mut wrong=entity.clone();
        let Value::Map(map)=&mut wrong.components[0].data else {panic!("bad fixture")};
        map.insert("weapon_type".into(),Value::UInt(255));
        let mut another=adapter.start_context(MODULE_ID,Id(102)).unwrap();
        assert!(another.instantiate(&wrong,None).is_err());
    }
}
