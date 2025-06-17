use glam::Vec2;
use math::{FixedVec2, f2v, fconst, fixed, fvec2, v2f};
use std::f32::consts::{FRAC_PI_4, PI};

fn main() {
    println!("=== FixedPoint vs f32 Comprehensive Demo ===\n");

    // Basic arithmetic operations
    println!("1. Basic Arithmetic:");
    let fp_a = fixed!(3.14159);
    let fp_b = fixed!(2.71828);
    let f_a = 3.14159f32;
    let f_b = 2.71828f32;

    println!(
        "   Addition:        Fixed: {} | f32: {}",
        fp_a + fp_b,
        f_a + f_b
    );
    println!(
        "   Multiplication:  Fixed: {} | f32: {}",
        fp_a * fp_b,
        f_a * f_b
    );
    println!(
        "   Division:        Fixed: {} | f32: {}",
        fp_a / fp_b,
        f_a / f_b
    );
    println!(
        "   Square root:     Fixed: {} | f32: {}",
        fp_a.sqrt(),
        f_a.sqrt()
    );

    // Trigonometric functions
    println!("\n2. Trigonometric Functions:");
    let angles = [0.0, PI / 6.0, PI / 4.0, PI / 3.0, PI / 2.0];
    for angle in angles {
        let fp_angle = fixed!(angle);
        println!(
            "   Angle {:.3} -> sin: Fixed: {:.6} | f32: {:.6}",
            angle,
            fp_angle.sin(),
            angle.sin()
        );
    }

    // Mathematical constants
    println!("\n3. Mathematical Constants:");
    println!("   PI:     Fixed: {} | f32: {}", fconst!(PI), PI);
    println!(
        "   E:      Fixed: {} | f32: {}",
        fconst!(E),
        std::f32::consts::E
    );
    println!(
        "   SQRT_2: Fixed: {} | f32: {}",
        fconst!(SQRT_2),
        std::f32::consts::SQRT_2
    );

    // Vector operations
    println!("\n4. Vector Operations:");
    let fv1 = fvec2!(3.0, 4.0);
    let fv2 = fvec2!(1.0, 2.0);
    let v1 = Vec2::new(3.0, 4.0);
    let v2 = Vec2::new(1.0, 2.0);

    println!("   Vector 1:        Fixed: {} | glam: {:?}", fv1, v1);
    println!("   Vector 2:        Fixed: {} | glam: {:?}", fv2, v2);
    println!(
        "   Addition:        Fixed: {} | glam: {:?}",
        fv1 + fv2,
        v1 + v2
    );
    println!(
        "   Dot product:     Fixed: {} | glam: {}",
        fv1.dot(fv2),
        v1.dot(v2)
    );
    println!(
        "   Length:          Fixed: {} | glam: {}",
        fv1.length(),
        v1.length()
    );
    println!(
        "   Normalized:      Fixed: {} | glam: {:?}",
        fv1.normalize(),
        v1.normalize()
    );

    // Advanced vector operations
    println!("\n5. Advanced Vector Operations:");
    let fv_base = fvec2!(1.0, 0.0);
    let v_base = Vec2::new(1.0, 0.0);

    let fv_rotated = fv_base.rotate(fconst!(FRAC_PI_2));
    println!(
        "   Rotate 90°:      Fixed: {} | Expected: (0, 1)",
        fv_rotated
    );

    let fv_from_angle = FixedVec2::from_angle(fconst!(FRAC_PI_4));
    let v_from_angle = Vec2::from_angle(FRAC_PI_4);
    println!(
        "   From angle 45°:  Fixed: {} | glam: {:?}",
        fv_from_angle, v_from_angle
    );

    // Conversion between types
    println!("\n6. Type Conversions:");
    let glam_vec = Vec2::new(2.5, -1.7);
    let fixed_vec = f2v!(glam_vec);
    let back_to_glam = v2f!(fixed_vec);

    println!("   Original glam:   {:?}", glam_vec);
    println!("   To FixedVec2:    {}", fixed_vec);
    println!("   Back to glam:    {:?}", back_to_glam);
    println!(
        "   Conversion error: x={:.6}, y={:.6}",
        (glam_vec.x - back_to_glam.x).abs(),
        (glam_vec.y - back_to_glam.y).abs()
    );

    // Gameplay simulation example
    println!("\n7. Gameplay Physics Simulation:");
    let mut player_pos = fvec2!(0.0, 0.0);
    let mut velocity = fvec2!(5.0, 3.0);
    let gravity = fvec2!(0.0, -9.8);
    let dt = fixed!(0.016667); // ~60 FPS

    println!("   Initial position: {}", player_pos);
    println!("   Initial velocity: {}", velocity);

    for frame in 1..=5 {
        velocity += gravity * dt;
        player_pos += velocity * dt;
        println!("   Frame {}: pos={}, vel={}", frame, player_pos, velocity);
    }

    // Performance comparison simulation
    println!("\n8. Precision Comparison:");
    let iterations = 1000;
    let mut fp_accumulator = fvec2!(0.0, 0.0);
    let mut f32_accumulator = Vec2::ZERO;
    let small_increment = fvec2!(0.001, 0.001);
    let small_increment_f32 = Vec2::new(0.001, 0.001);

    for _ in 0..iterations {
        fp_accumulator += small_increment;
        f32_accumulator += small_increment_f32;
    }

    println!(
        "   After {} iterations of adding (0.001, 0.001):",
        iterations
    );
    println!("   Fixed result:    {}", fp_accumulator);
    println!("   f32 result:      {:?}", f32_accumulator);
    println!("   Expected:        (1.000, 1.000)");

    // Collision detection example
    println!("\n9. Collision Detection:");
    let circle_center = fvec2!(5.0, 5.0);
    let circle_radius = fixed!(2.0);
    let test_points = [
        fvec2!(5.0, 5.0), // Center
        fvec2!(6.5, 5.0), // Inside
        fvec2!(8.0, 5.0), // Outside
    ];

    for (i, point) in test_points.iter().enumerate() {
        let distance = circle_center.distance(*point);
        let is_inside = distance <= circle_radius;
        println!(
            "   Point {}: {} -> distance={}, inside={}",
            i + 1,
            point,
            distance,
            is_inside
        );
    }

    println!("\n=== Demo Complete ===");
}
