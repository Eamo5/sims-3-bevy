//! Which of the game's ~35,000 animation clips to bake: walks and idles, the start/loop/stop
//! clips of every supported object interaction, two-Sim social animations (both sides) and
//! their child equivalents. Clips are stored lz4-compressed in `clips.pack`, keyed by the
//! FNV-64 of the lowercased name, and decoded lazily by the game.

/// Clip-name prefixes to bake. Object and social clips must also end in `_x` / `_y` (the
/// Sim's side; other suffixes are the props').
pub const CLIP_PREFIXES: &[&str] = &[
    // Walks and idles.
    "a_female_walk",
    "a_male_walk",
    "a_idle_neutral_",
    "a_idle_friendly_",
    "a_idle_wipeSweat",
    "a_idle_fanSelf",
    "a_dance_beg_",
    "a_dance_med_",
    "c_walk",
    "c_idle_neutral_",
    "c_idleBlend",
    // Kitchen.
    "a2o_fridge_openDoor",
    "a2o_fridge_closeDoor",
    "a2o_fridge_cookSomthing",
    "a2o_stove_fryingPan_start_fromNeutral",
    "a2o_stove_fryingPan_idle",
    "a2o_stove_fryingPan_spatula",
    "a2o_stove_fryingPan_pourPlate_x",
    "a2o_stove_clean",
    "a2o_eat_stand_fork",
    "a2o_eat_stand_hand",
    // Bathroom.
    "a2o_toilet_useStanding",
    "a2o_toilet_flush_x",
    "a2o_shower_takeShower_getIn",
    "a2o_shower_takeShower_loop",
    "a2o_shower_takeShower_getOut",
    "a2o_bathtub_relax",
    "a2o_sink_washhands",
    "a2o_sink_brushTeeth",
    // Beds and seating.
    "a2o_bed_getIn_made",
    "a2o_bed_getout",
    "a2o_bed_nap",
    "a2o_bed_sleep",
    "a2o_bed_trans_relax2sleep",
    "a2o_bed_relax_loop",
    "a2o_bed_relax_getin",
    "a2o_chairLiving_sit_breathe",
    "a2o_chairLiving_sit_crossedLeg_front",
    "a2o_sitTemplate_sit_loopBreathe",
    "a2o_sofa_",
    // Electronics, hobbies, skills.
    "a2o_tv_watch_",
    "a2o_computer_game_loop",
    "a2o_computer_chess_type_loop",
    "a2o_book_readBook_standing",
    "a2o_mirror_full_checkSelfOut",
    "a2o_painting_start",
    "a2o_painting_loopMed",
    "a2o_painting_loopLarge",
    "a2o_painting_consider",
    "a2o_painting_stop",
    "a2o_holographicEasel_loopMed",
    "a2o_guitar_play_",
    "a2o_treadmill_jog",
    "a2o_chessTable_loop",
    "a2o_chessTable_move",
    "a2o_phone_call_loopBreathe",
    "a2o_phone_chat_talk",
    // Socials (both sides).
    "a2a_soc_Neutral_Gossip_Friendly_Neutral",
    "a2a_soc_Neutral_RambleAimlessly_talk",
    "a2a_soc_neutral_tellJoke_accept",
    "a2a_soc_neutral_tellJoke_reject",
    "a2a_soc_Neutral_Compliment_Friendly",
    "a2a_soc_Neutral_Compliment_Amorous",
    "a2a_soc_friendly_hug_accept",
    "a2a_soc_Neutral_FriendlyHug_Friendly_Neutral",
    "a2a_soc_neutral_highFive_friendly_neutral",
    "a2a_soc_neutral_probe_tickle",
    "a2a_soc_Neutral_Flirt_Neutral_Neutral",
    "a2a_soc_Neutral_Flirt_Amorous_Amorous",
    "a2a_soc_Amorous_HoldHands_Affectionate_Amorous",
    "a2a_soc_Amorous_ShyKiss_Amorous_Amorous",
    "a2a_soc_amorous_kissMakeOut_accept",
    "a2a_soc_amorous_kissRomantic_romantic_amorous",
    "a2a_soc_Amorous_ProposeMarriage_Amorous_Amorous",
    "a2a_soc_Amorous_ProposeWedding_Accept",
    "a2a_soc_Amorous_Wedding_Amorous_Amorous",
    "a2a_soc_Bad_Mock_Insulting_Bad",
    "a2a_soc_Bad_Accuse_Insulting_Bad",
    "a2a_soc_Bad_Slap_Steamed_Bad",
    "a2a_soc_Neutral_BreakUp_Neutral_Neutral",
    "a2a_bed_cuddle_woohoo",
    "a2a_danceClub_dance_medSkill_loop1",
    // Children.
    "c2o_bed_getIn_made",
    "c2o_bed_getout",
    "c2o_bed_nap",
    "c2o_bed_sleep",
    "c2o_toilet",
    "c2o_shower_takeShower",
    "c2o_fridge_openDoor",
    "c2o_tv_watch_",
    "c2o_computer_game_loop",
    // Toddlers.
    "p_walk",
    "p_idle_neutral_loop",
    "p_idle_friendly_loop",
    "p_idle_breathe_x",
    "p_crawl_x",
    "p2o_crib_",
    "p2o_toyXylophone_play_",
    "p2o_toyPegBox_play_",
    "p2o_toybox_playIn_",
    "p2o_highChair_",
    // Babies.
    "b_idle_breathe",
    "b_sleep_beingHeld_y",
    "b_motDistress_cry_carry",
    "b2o_crib_",
    // Grown-ups looking after them.
    "a2b_crib_putIn",
    // More things to do at home.
    "a2o_dresser_use_",
    "c2o_dresser_use_",
    "a2o_telescope_",
    "a2o_swingset_",
    "c2o_swingset_",
    "a2o_hotTub_",
    "c2o_dollhouse_play_",
    "c2o_JungleGym",
    "a2o_foosballTable_play",
    "a2b_crib_pullOut",
    "a2b_babyBottle_feed",
    "a2b_changeDiaper",
    "a2b_carryPose",
    "a2b_idle_carry_",
    "a2b_pickUp",
    "a2p_idle_carry_",
    "a2p_carryPose",
    "a2p_crib_putIn",
    "a2p_crib_pullOut",
    "a2p_carry_chat_",
    "a2p_pickUp",
    "a2p_changeDiaper",
    "a2p_highChair_",
    "a2p_book_readWith_",
    "a2p_babyBottle_giveTake",
];

/// Whether a clip should be baked.
pub fn wanted(name: &str) -> bool {
    let actor_side = |n: &str| n.ends_with("_x") || n.ends_with("_y");
    CLIP_PREFIXES.iter().any(|p| {
        if !name.starts_with(p) {
            return false;
        }
        let object_or_social = ["a2o_", "a2a_", "c2o_", "p2o_", "b2o_", "a2b_", "a2p_"].iter().any(|p| name.starts_with(p));
        !object_or_social || actor_side(name)
    })
}

/// Pack key of a clip by name.
pub fn clip_key(name: &str) -> crate::types::Key {
    (s3pkg::types::CLIP, 0, s3pkg::fnv64(&name.to_ascii_lowercase()))
}
