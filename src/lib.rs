mod atomic_bit_set;

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::ops::{Deref, DerefMut};

pub struct World {
    entities: HashMap<EntityId, Entity>,
    entity_cnt: NonZeroUsize,
}

impl World {

    pub fn new_entity(&mut self) -> &mut Entity {
        let id = self.entity_cnt;
        self.entity_cnt = id.checked_add(1).unwrap();
        self.entities.entry(id).or_insert(Entity {
            id,
            components: HashMap::new(),
        })
    }

}

impl Default for World {
    fn default() -> Self {
        Self {
            entities: Default::default(),
            entity_cnt: NonZeroUsize::new(1).unwrap(),
        }
    }
}

pub struct Entity {
    id: NonZeroUsize,
    components: HashMap<TypeId, Box<dyn Any>>,
}

impl Entity {

    #[inline(always)]
    pub fn id(&self) -> NonZeroUsize {
        self.id
    }

    pub fn add_component<CT: 'static>(&mut self, component: CT) {
        self.components.insert(TypeId::of::<CT>(), Box::new(component));
    }

    pub fn remove_component<CT: 'static>(&mut self) -> Option<Box<CT>> {
        self.components.remove(&TypeId::of::<CT>()).map(|val| val.downcast::<CT>().unwrap())
    }

    pub fn get_component<CT: 'static>(&self) -> Option<&CT> {
        self.components.get(&TypeId::of::<CT>()).map(|val| val.downcast_ref::<CT>().unwrap())
    }

    pub fn get_component_mut<CT: 'static>(&mut self) -> Option<&mut CT> {
        self.components.get_mut(&TypeId::of::<CT>()).map(|val| val.downcast_mut::<CT>().unwrap())
    }

}

pub type EntityId = NonZeroUsize;

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Copy, Clone)]
    struct Health {
        value: f64,
    }

    #[test]
    fn insertion() {
        let mut world = World::default();
        let mut entity = world.new_entity();
        entity.add_component(Health {
            value: 20.0,
        });
        assert_eq!(*entity.get_component::<Health>().unwrap(), Health {
            value: 20.0,
        });
    }
}
