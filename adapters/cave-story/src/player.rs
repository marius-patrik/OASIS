//! Original, upstream Cave Story player simulation feasibility gate.
//! No gameplay physics are copied into OASIS.

#[cfg(test)]
mod tests {
    use doukutsu_rs::{
        entity::GameEntity,
        framework::context::Context,
        game::{
            filesystem_container::FilesystemContainer,
            npc::list::NPCList,
            player::Player,
            shared_game_state::SharedGameState,
        },
    };

    #[test]
    fn upstream_player_ticks_in_original_headless_shared_game_context() {
        let mut context=Context::new();
        context.headless=true;
        let mut filesystem=FilesystemContainer::new();
        filesystem.mount_fs(&mut context).expect("mount native built-in resources");
        let mut state=SharedGameState::new(&mut context)
            .expect("initialize original Cave Story runtime in headless mode");
        let mut player=Player::new(&mut state,&mut context);
        player.cond.set_alive(true);
        state.control_flags.set_control_enabled(true);
        let (npcs,_token)=NPCList::new();
        let initial=(player.x,player.y,player.vel_x,player.vel_y);
        player.tick(&mut state,&npcs).expect("actual native Player::tick");
        let subsequent=(player.x,player.y,player.vel_x,player.vel_y);
        assert_ne!(initial,subsequent,
            "native game tick must update the original character's movement state");
    }
}
