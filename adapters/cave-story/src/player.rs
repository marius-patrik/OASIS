//! Integration test for the actual original upstream game Player::tick,
//! not a home-grown movement/collision model.
#[cfg(test)]
mod tests {
    use doukutsu_rs::oasis_bridge::{Buttons, Simulation};

    #[test]
    fn source_player_tick_responds_to_native_movement_input() {
        let mut simulation=Simulation::new()
            .expect("initialize the original Cave Story game resources in headless mode");
        let start=simulation.frame();
        simulation.controls(Buttons{right:true,..Buttons::default()});
        for _ in 0..8 {
            simulation.tick().expect("original Player::tick must execute");
        }
        let moving=simulation.frame();
        assert!(moving.x>start.x,
            "upstream Cave Story native movement should respond to the right input");
        assert!(moving.vel_x>0,
            "upstream original movement should accelerate the player");
    }

    #[test]
    fn original_weapon_and_bullet_system_fires_on_source_player_input() {
        use doukutsu_rs::game::weapon::WeaponType;
        let mut sim=Simulation::new().expect("original headless engine");
        let (pxm,attrib)=flat_native_stage();
        sim.load_stage(&pxm,&attrib).expect("native original PXM reader");
        sim.player.x=6*8192;
        sim.player.y=2*8192;
        sim.equip_weapon(WeaponType::PolarStar,3);
        assert_eq!(sim.current_weapon_ammo(),Some((3,3)));
        sim.controls(Buttons{shoot:true,..Buttons::default()});
        sim.tick().expect("upstream original weapon and bullet tick");
        assert!(sim.active_bullets()>0,
            "original weapon logic should spawn an original-engine projectile");
        assert_eq!(sim.current_weapon_ammo(),Some((2,3)),
            "the original game must consume its own source-native ammunition");
    }

    fn flat_native_stage() -> (Vec<u8>,Vec<u8>) {
        // A 12x12 Cave Story PXM v0x10 world with original solid attribute
        // 0x41 across the entire seventh row. No proprietary stage data.
        const WIDTH:usize=12;
        const HEIGHT:usize=12;
        let mut tiles=vec![0u8;WIDTH*HEIGHT];
        for x in 0..WIDTH {tiles[7*WIDTH+x]=1;}
        let mut pxm=b"PXM".to_vec();
        pxm.push(0x10);
        pxm.extend_from_slice(&(WIDTH as u16).to_le_bytes());
        pxm.extend_from_slice(&(HEIGHT as u16).to_le_bytes());
        pxm.extend_from_slice(&tiles);
        let mut attrs=vec![0u8;256];
        attrs[1]=0x41;
        (pxm,attrs)
    }

    #[test]
    fn original_tile_collision_stops_native_player_at_solid_ground() {
        let mut simulation=Simulation::new().expect("native game initialization");
        let (pxm,attrs)=flat_native_stage();
        simulation.load_stage(&pxm,&attrs).expect("upstream PXM map loader");
        simulation.player.x=6*8192; // center of the generated native map
        simulation.player.y=2*8192;
        let mut grounded=false;
        for _ in 0..160 {
            let frame=simulation.tick().expect("actual native movement and collision tick");
            if frame.collision_flags & 0x8 != 0 {
                grounded=true;
                break;
            }
        }
        assert!(grounded,
            "the original PhysicalEntity::tick_map_collisions must ground the player");
    }

    #[test]
    fn source_stage_input_validation_precedes_native_physics_allocation() {
        let mut simulation=Simulation::new().expect("native game initialization");
        let (mut pxm,attributes)=flat_native_stage();
        pxm[4]=255;
        pxm[5]=255;
        assert!(simulation.load_stage(&pxm,&attributes).is_err());
        assert!(simulation.tick().is_ok(),
            "invalid source stage must not destroy independent player simulation");
    }

    #[test]
    fn native_headless_reverse_direction_exercises_original_player_control() {
        let mut simulation=Simulation::new().expect("original game init");
        simulation.controls(Buttons{right:true,..Buttons::default()});
        for _ in 0..8 {simulation.tick().unwrap();}
        let moving_right=simulation.frame();
        simulation.controls(Buttons{left:true,..Buttons::default()});
        for _ in 0..8 {simulation.tick().unwrap();}
        let reversing=simulation.frame();
        assert!(moving_right.vel_x>0);
        assert!(reversing.vel_x<moving_right.vel_x,
            "original native player tick must respond to reversed directional input");
    }
}
