use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};

const FLOOR_Y: f32 = -360.0;
const PLAYER_RADIUS: f32 = 25.0;

fn main() {
    // When building for WASM, print panics to the browser console
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(PhysicsPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, close_on_esc)
        .run();
}

fn close_on_esc(
    mut commands: Commands,
    focused_windows: Query<(Entity, &Window)>,
    input: Res<ButtonInput<KeyCode>>,
) {
    for (window, _focus) in focused_windows.iter() {
        if input.just_pressed(KeyCode::Escape) {
            commands.entity(window).despawn();
        }
    }
}

struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        // Chaining should ensure the correct execution order
        app.add_systems(
            FixedUpdate,
            (
                player_impulse_system,
                integrator_before_system,
                (gravity_system, player_force_system).after(integrator_before_system),
                friction_force_system,
                integrator_after_system,
                collision_system,
                friction_impulse_system,
            )
                .chain(),
        );
    }
}

#[derive(Component)]
struct PhysObj {
    mass: f32,
    vel: Vec2,
    acc: Vec2,
    acc_prev: Vec2,
    moment_of_inertia: f32,
    angular_vel: f32,
    angular_acc: f32,
    angular_acc_prev: f32,
}

#[derive(Component)]
struct Gravity(f32);

impl Default for Gravity {
    fn default() -> Self {
        Gravity(2000.0)
    }
}

#[derive(Component)]
enum Collider {
    Ball {
        radius: f32,
        coef_of_restitution: f32,
        touching_ground: bool,
        kinetic_friction: f32,
        friction_acc: f32,
        friction_acc_prev: f32,
    },
}

#[derive(Component)]
struct Player {
    jump_impulse: f32,
    torque: f32,
}

struct FidgetSpinner {
    radius: f32,
    bump_size: f32,
    bumps: u32,
    vertices: usize,
}

impl Default for FidgetSpinner {
    fn default() -> Self {
        Self {
            radius: 0.5,
            bump_size: 0.5 / 16.0,
            bumps: 12,
            vertices: 24,
        }
    }
}

impl FidgetSpinner {
    pub fn new(radius: f32) -> Self {
        Self {
            radius,
            bump_size: radius / 16.0,
            ..Default::default()
        }
    }
}

impl From<FidgetSpinner> for Mesh {
    fn from(shape: FidgetSpinner) -> Self {
        let FidgetSpinner {
            radius,
            bump_size,
            bumps,
            vertices,
        } = shape;
        let mut positions = Vec::with_capacity(vertices + 1);
        let mut normals = Vec::with_capacity(vertices + 1);

        positions.push([0.0, 0.0, 0.0]);
        normals.push([0.0, 0.0, 1.0]);

        let step = std::f32::consts::TAU / vertices as f32;
        for i in 0..vertices {
            let theta = i as f32 * step;
            let (sin, cos) = theta.sin_cos();
            let offset = bump_size * f32::cos(bumps as f32 * theta);
            positions.push([cos * (radius + offset), sin * (radius + offset), 0.0]);
            normals.push([0.0, 0.0, 1.0]);
        }

        let mut indices = Vec::with_capacity(vertices * 3);
        for i in 1..=(vertices as u32 - 1) {
            indices.extend_from_slice(&[0, i, i + 1]);
        }
        indices.extend_from_slice(&[0, vertices as u32, 1]);

        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_indices(Indices::U32(indices));
        mesh
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn(Camera2d);

    // Player ball
    commands.spawn((
        Mesh2d(meshes.add(FidgetSpinner::new(PLAYER_RADIUS))),
        MeshMaterial2d(materials.add(Color::from(Srgba::BLUE))),
        Transform::from_xyz(0.0, 0.0, 0.0),
        PhysObj {
            mass: 10.0,
            vel: Vec2::ZERO,
            acc: Vec2::ZERO,
            acc_prev: Vec2::ZERO,
            moment_of_inertia: 10.0 * 0.5 * PLAYER_RADIUS.powi(2),
            angular_vel: 0.0,
            angular_acc: 0.0,
            angular_acc_prev: 0.0,
        },
        Gravity::default(),
        Collider::Ball {
            radius: PLAYER_RADIUS,
            coef_of_restitution: 0.3,
            touching_ground: false,
            kinetic_friction: 0.5,
            friction_acc: 0.0,
            friction_acc_prev: 0.0,
        },
        Player {
            jump_impulse: 10_000.0,
            torque: 200_000.0,
        },
    ));
}

fn gravity_system(mut query: Query<(&mut PhysObj, &Gravity)>) {
    for (mut phys_obj, gravity) in &mut query {
        phys_obj.acc += Vec2::NEG_Y * gravity.0;
    }
}

fn player_impulse_system(
    mut commands: Commands,
    input: Res<ButtonInput<KeyCode>>,
    mut query: Query<(Entity, &Player, &mut PhysObj, &mut Collider)>,
) {
    for (entity, player, mut phys_obj, mut collider) in &mut query {
        let Collider::Ball {
            touching_ground, ..
        } = &mut *collider;

        if input.pressed(KeyCode::Space) && *touching_ground {
            let dv = Vec2::Y * player.jump_impulse / phys_obj.mass;
            phys_obj.vel += dv;
        }

        if input.just_pressed(KeyCode::KeyK) {
            commands.entity(entity).remove::<Gravity>();
        }
        if input.just_released(KeyCode::KeyK) {
            commands.entity(entity).insert(Gravity::default());
        }
    }
}

fn player_force_system(
    input: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&Player, &mut PhysObj)>,
) {
    for (player, mut phys_obj) in &mut query {
        if input.pressed(KeyCode::KeyA) {
            phys_obj.angular_acc += player.torque / phys_obj.moment_of_inertia;
        }
        if input.pressed(KeyCode::KeyD) {
            phys_obj.angular_acc -= player.torque / phys_obj.moment_of_inertia;
        }
    }
}

// The part of the integrator that runs before applying forces
fn integrator_before_system(time: Res<Time>, mut query: Query<(&mut Transform, &mut PhysObj)>) {
    let dt = time.delta_secs();
    for (mut transform, mut phys_obj) in &mut query {
        integrate_before(dt, &mut transform, &mut phys_obj);
    }
}

// The part of the integrator that runs after applying forces
fn integrator_after_system(time: Res<Time>, mut query: Query<&mut PhysObj>) {
    let dt = time.delta_secs();
    for mut phys_obj in &mut query {
        integrate_after(dt, &mut phys_obj);
    }
}

fn integrate_before(dt: f32, transform: &mut Transform, phys_obj: &mut PhysObj) {
    phys_obj.vel += 0.5 * phys_obj.acc * dt;
    transform.translation += (phys_obj.vel * dt).extend(0.0);
    phys_obj.acc_prev = phys_obj.acc;
    // Functions that calculate acceleration simply add to it so it must be reset every iteration.
    phys_obj.acc = Vec2::ZERO;

    phys_obj.angular_vel += 0.5 * phys_obj.angular_acc * dt;
    transform.rotate_z(phys_obj.angular_vel * dt);
    phys_obj.angular_acc_prev = phys_obj.angular_acc;
    phys_obj.angular_acc = 0.0;
}

fn integrate_after(dt: f32, phys_obj: &mut PhysObj) {
    phys_obj.vel += 0.5 * phys_obj.acc * dt;
    phys_obj.angular_vel += 0.5 * phys_obj.angular_acc * dt;
}

// Integrator for when acceleration is assumed constant (used in collision resolving)
fn integrate_simple(dt: f32, transform: &mut Transform, phys_obj: &mut PhysObj) {
    let dv = phys_obj.acc * dt;
    transform.translation += ((phys_obj.vel + 0.5 * dv) * dt).extend(0.0);
    phys_obj.vel += dv;

    let dav = phys_obj.angular_acc * dt;
    transform.rotate_z((phys_obj.angular_vel + 0.5 * dav) * dt);
    phys_obj.angular_vel += dav;
}

fn collision_system(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &mut PhysObj, &mut Collider)>,
) {
    let dt = time.delta_secs();
    for (mut transform, mut phys_obj, mut collider) in &mut query {
        match *collider {
            Collider::Ball {
                radius,
                ref mut touching_ground,
                ..
            } => {
                if transform.translation.y - radius <= FLOOR_Y {
                    while resolve_collision(dt, &mut transform, &mut phys_obj, &mut collider) {}
                } else {
                    *touching_ground = false;
                }
            }
        }
    }
}

fn resolve_collision(
    dt: f32,
    transform: &mut Transform,
    phys_obj: &mut PhysObj,
    collider: &mut Collider,
) -> bool {
    match collider {
        Collider::Ball {
            radius,
            touching_ground: true,
            ..
        } => {
            transform.translation.y = FLOOR_Y + *radius;
            phys_obj.vel.y = 0.0;
            false
        }
        Collider::Ball {
            radius,
            coef_of_restitution,
            touching_ground,
            kinetic_friction,
            ..
        } => {
            *touching_ground = true;
            bounce(
                dt,
                transform,
                phys_obj,
                *radius,
                *coef_of_restitution,
                *kinetic_friction,
            )
        }
    }
}

fn bounce(
    dt: f32,
    transform: &mut Transform,
    phys_obj: &mut PhysObj,
    radius: f32,
    coef_of_restitution: f32,
    kinetic_friction: f32,
) -> bool {
    let collision_dt = calculate_collision_dt(
        (transform.translation.y - radius) - FLOOR_Y,
        phys_obj.vel.y,
        phys_obj.acc.y,
    );

    if collision_dt > 0.5 * dt || collision_dt.is_nan() {
        integrate_simple(-0.5 * dt, transform, phys_obj);
        std::mem::swap(&mut phys_obj.acc, &mut phys_obj.acc_prev);

        let collision_dt2 = calculate_collision_dt(
            (transform.translation.y - radius) - FLOOR_Y,
            phys_obj.vel.y,
            phys_obj.acc.y,
        );
        assert!(collision_dt2 >= 0.0);

        integrate_simple(-collision_dt2, transform, phys_obj);
        apply_friction_impulse(
            phys_obj,
            radius,
            -phys_obj.vel.y * (1.0 + coef_of_restitution),
            kinetic_friction,
            0.0,
        );
        phys_obj.vel.y *= -coef_of_restitution;
        integrate_simple(collision_dt2, transform, phys_obj);

        std::mem::swap(&mut phys_obj.acc, &mut phys_obj.acc_prev);
        integrate_simple(0.5 * dt, transform, phys_obj);
    } else {
        assert!(collision_dt >= 0.0);
        integrate_simple(-collision_dt, transform, phys_obj);

        let normal_impulse = -phys_obj.vel.y * (1.0 + coef_of_restitution);
        apply_friction_impulse(phys_obj, radius, normal_impulse, kinetic_friction, 0.0);
        phys_obj.vel.y *= -coef_of_restitution;

        integrate_simple(collision_dt, transform, phys_obj);
    }
    false
    // TODO: Calculate time until bouncing stops and proceed as follows:
    //    - If that time is less than the time step, approximate behavior that results in
    //        the ball laying/sliding on the ground at the end of the frame.
    //    - Otherwise, bounce once and return whether another bounce will happen during the frame.
    // Written out this seems like a bad way to do it... That's a problem for another day.
}

fn calculate_collision_dt(s: f32, v: f32, a: f32) -> f32 {
    if a.abs() < f32::EPSILON {
        s / v
    } else {
        (v - (v.powi(2) - 2.0 * a * s).sqrt().copysign(v)) / a
    }
}

fn apply_friction_impulse(
    phys_obj: &mut PhysObj,
    radius: f32,
    normal_impulse: f32,
    kinetic_friction: f32,
    applied_friction: f32, // friction that has already been applied earlier in the frame
) {
    let relative_speed = phys_obj.vel.x + phys_obj.angular_vel * radius;
    let max_impulse =
        normal_impulse * kinetic_friction + applied_friction * relative_speed.signum();
    let stopping_impulse = phys_obj.moment_of_inertia * relative_speed.abs()
        / (phys_obj.mass * radius.powi(2) + phys_obj.moment_of_inertia);
    let impulse = f32::min(max_impulse, stopping_impulse).copysign(-relative_speed);

    phys_obj.vel.x += impulse;
    phys_obj.angular_vel += impulse * phys_obj.mass * radius / phys_obj.moment_of_inertia;
}

fn friction_impulse_system(time: Res<Time>, mut query: Query<(&mut PhysObj, &Collider)>) {
    let dt = time.delta_secs();
    for (mut phys_obj, collider) in &mut query {
        if let Collider::Ball {
            radius,
            touching_ground: true,
            kinetic_friction,
            friction_acc,
            friction_acc_prev,
            ..
        } = *collider
        {
            if phys_obj.vel.y.abs() < 0.1 {
                let normal_impulse = -(phys_obj.acc.y + phys_obj.acc_prev.y) * 0.5 * dt;
                let applied_friction = (friction_acc + friction_acc_prev) * 0.5 * dt;
                apply_friction_impulse(
                    &mut phys_obj,
                    radius,
                    normal_impulse,
                    kinetic_friction,
                    applied_friction,
                );
            }
        }
    }
}

fn friction_force_system(mut query: Query<(&mut PhysObj, &mut Collider)>) {
    for (mut phys_obj, mut collider) in &mut query {
        if let Collider::Ball {
            radius,
            touching_ground: true,
            kinetic_friction,
            ref mut friction_acc,
            ref mut friction_acc_prev,
            ..
        } = *collider
        {
            let normal_force = -phys_obj.acc.y;
            let rel_acc = phys_obj.acc.x + phys_obj.angular_acc * radius;
            let max_f = normal_force * kinetic_friction;
            let stop_f = phys_obj.moment_of_inertia * rel_acc.abs()
                / (phys_obj.mass * radius.powi(2) + phys_obj.moment_of_inertia);
            let force = f32::min(max_f, stop_f).copysign(-rel_acc);

            *friction_acc_prev = *friction_acc;
            *friction_acc = force;
            phys_obj.acc.x += force;
            phys_obj.angular_acc += force * phys_obj.mass * radius / phys_obj.moment_of_inertia;
        }
    }
}
