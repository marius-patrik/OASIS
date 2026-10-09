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
