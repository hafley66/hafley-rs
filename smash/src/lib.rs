pub mod fighters {
    #[path = "0_controller.rs"]
    pub mod controller;

    #[path = "pigeon/2_simulation.rs"]
    pub mod pigeon;

    #[path = "dog/3_movement.rs"]
    pub mod dog;
}
