//! Independently registered native engine adapters. Catalog entries are global
//! declarations; each world instantiates its own execution contexts. No
//! adapter ever imports another game or writes the universal database itself.

use std::collections::BTreeMap;
use oasis_contracts::{
    AdapterRegistry, ContractError, ContractResult, DefinitionRef, GameAdapter,
    Id, ModuleDescriptor, TypeRef,
};
use crate::universe::Universe;

fn invalid(message: &str) -> ContractError {
    ContractError::InvalidData(message.into())
}

type CapabilityKey = (String, String, u32);
fn key(capability: &TypeRef) -> CapabilityKey {
    (capability.namespace.clone(), capability.name.clone(), capability.version)
}

#[derive(Clone, Default)]
pub struct Catalog {
    modules: BTreeMap<Id, ModuleDescriptor>,
    definitions: BTreeMap<Id, DefinitionRef>,
    capabilities: BTreeMap<CapabilityKey, Id>,
}

impl Catalog {
    pub fn new() -> Self { Self::default() }

    /// All-or-nothing registration. One adapter's failed or colliding
    /// declarations cannot corrupt previously installed adapters.
    pub fn install(&mut self, adapter: &dyn GameAdapter) -> ContractResult<()> {
        let mut candidate = self.clone();
        adapter.register(&mut candidate)?;
        *self = candidate;
        Ok(())
    }

    pub fn module(&self, id: Id) -> Option<&ModuleDescriptor> {
        self.modules.get(&id)
    }
    pub fn definition(&self, id: Id) -> Option<&DefinitionRef> {
        self.definitions.get(&id)
    }
    pub fn capability_provider(&self, capability: &TypeRef) -> Option<Id> {
        self.capabilities.get(&key(capability)).copied()
    }

    /// A world selects which native modules it actually hosts. The adapter
    /// creates a new native simulation instance with a fresh context ID.
    pub fn activate(
        &self,
        adapter: &dyn GameAdapter,
        universe: &mut Universe,
        world: Id,
        module: Id,
        context: Id,
    ) -> ContractResult<()> {
        let declared = self.modules.get(&module)
            .ok_or(ContractError::NotFound(module))?;
        let executable = adapter.start_context(module, context)?;
        let actual = executable.descriptor();
        if actual.module_id != declared.module_id ||
           actual.engine_id != declared.engine_id ||
           actual.contract_major != declared.contract_major ||
           actual.contract_minor < declared.contract_minor {
            return Err(invalid("native module does not match registered adapter declaration"));
        }
        universe.add_module(world,context,executable)
    }
}

impl AdapterRegistry for Catalog {
    fn register_module(&mut self, descriptor: ModuleDescriptor) -> ContractResult<()> {
        if descriptor.contract_major != 0 || descriptor.contract_minor < 1 {
            return Err(invalid("unsupported module contract version"));
        }
        if self.modules.contains_key(&descriptor.module_id) {
            return Err(invalid("duplicate module identifier"));
        }
        self.modules.insert(descriptor.module_id,descriptor);
        Ok(())
    }
    fn register_definition(&mut self, definition: DefinitionRef) -> ContractResult<()> {
        if definition.version == 0 {
            return Err(invalid("definition version must be positive"));
        }
        if self.definitions.contains_key(&definition.id) {
            return Err(invalid("duplicate definition identifier"));
        }
        self.definitions.insert(definition.id,definition);
        Ok(())
    }
    fn register_capability(&mut self, capability: TypeRef, module_id: Id)
        -> ContractResult<()> {
        if !self.modules.contains_key(&module_id) {
            return Err(ContractError::NotFound(module_id));
        }
        if capability.namespace.is_empty() || capability.name.is_empty() ||
            capability.version == 0 {
            return Err(invalid("invalid capability type reference"));
        }
        if self.capabilities.insert(key(&capability),module_id).is_some() {
            return Err(invalid("duplicate capability declaration"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use oasis_contracts::{
        ClockStep, EntityView, InputIntent, NativeHandle, NativeModule,
        Revision, Snapshot, StepOutput, WorldPort,
    };

    struct IndependentAdapter { engine: Id, module: Id }
    impl GameAdapter for IndependentAdapter {
        fn register(&self, registry: &mut dyn AdapterRegistry) -> ContractResult<()> {
            registry.register_module(ModuleDescriptor {
                engine_id:self.engine,module_id:self.module,
                contract_major:0,contract_minor:1,
                exported_interfaces:vec![],required_interfaces:vec![],
            })?;
            registry.register_definition(DefinitionRef{id:self.module,version:1})?;
            registry.register_capability(TypeRef {
                namespace:format!("native.{}",self.module.0),
                name:"move".into(),version:1,
            },self.module)
        }
        fn start_context(&self, module_id: Id, context_id: Id)
            -> ContractResult<Box<dyn NativeModule>> {
            if self.module != module_id {return Err(ContractError::NotFound(module_id))}
            Ok(Box::new(NativeFixture{
                engine:self.engine,module:self.module,context:context_id,
                next:0,instances:BTreeMap::new(),
            }))
        }
    }

    struct NativeFixture {
        engine:Id,module:Id,context:Id,next:u64,
        instances:BTreeMap<u64,Snapshot>,
    }
    impl NativeModule for NativeFixture {
        fn descriptor(&self) -> ModuleDescriptor {
            ModuleDescriptor{
                engine_id:self.engine,module_id:self.module,
                contract_major:0,contract_minor:1,
                exported_interfaces:vec![],required_interfaces:vec![],
            }
        }
        fn instantiate(&mut self, entity:&EntityView, state:Option<&Snapshot>)
            -> ContractResult<NativeHandle> {
            self.next+=1;
            self.instances.insert(self.next,state.cloned().unwrap_or(Snapshot{
                entity_id:entity.id,state:vec![],revision:Revision(0),
                binary_artifact:None,
            }));
            Ok(NativeHandle{context_id:self.context,native_slot:self.next})
        }
        fn step(&mut self, _clock:ClockStep, _inputs:&[InputIntent],
                _world:&mut dyn WorldPort) -> ContractResult<StepOutput> {
            Ok(StepOutput{state_changes:vec![],interactions:vec![],emitted_events:vec![]})
        }
        fn snapshot(&self, handle:NativeHandle) -> ContractResult<Snapshot> {
            self.instances.get(&handle.native_slot).cloned()
                .ok_or(ContractError::NotFound(self.context))
        }
        fn restore(&mut self, handle:NativeHandle, snapshot:&Snapshot)
            -> ContractResult<()> {
            *self.instances.get_mut(&handle.native_slot)
                .ok_or(ContractError::NotFound(self.context))?=snapshot.clone();
            Ok(())
        }
        fn remove(&mut self, handle:NativeHandle) -> ContractResult<()> {
            self.instances.remove(&handle.native_slot)
                .ok_or(ContractError::NotFound(self.context))?;
            Ok(())
        }
    }

    #[test]
    fn independently_registered_adapters_share_one_universal_catalog() {
        let first=IndependentAdapter{engine:Id(10),module:Id(11)};
        let second=IndependentAdapter{engine:Id(20),module:Id(21)};
        let mut catalog=Catalog::new();
        catalog.install(&first).unwrap();
        catalog.install(&second).unwrap();
        let mut universe=Universe::new();
        universe.add_world(Id(100)).unwrap();
        universe.add_world(Id(200)).unwrap();
        catalog.activate(&first,&mut universe,Id(100),Id(11),Id(101)).unwrap();
        catalog.activate(&first,&mut universe,Id(200),Id(11),Id(201)).unwrap();
        catalog.activate(&second,&mut universe,Id(200),Id(21),Id(202)).unwrap();
        assert_eq!(universe.world_count(),2);
        assert_eq!(catalog.capability_provider(&TypeRef {
            namespace:"native.21".into(),name:"move".into(),version:1,
        }),Some(Id(21)));
    }

    #[test]
    fn failed_registration_does_not_mutate_existing_catalog() {
        let first=IndependentAdapter{engine:Id(10),module:Id(11)};
        let duplicate=IndependentAdapter{engine:Id(20),module:Id(11)};
        let mut catalog=Catalog::new();
        catalog.install(&first).unwrap();
        assert!(catalog.install(&duplicate).is_err());
        assert_eq!(catalog.module(Id(11)).unwrap().engine_id,Id(10));
        assert!(catalog.definition(Id(11)).is_some());
    }
}
