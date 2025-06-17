use math::{FixedPoint, FixedVec2};

fn main() {
    let a = FixedPoint::from(3.14159);
    let b = FixedPoint::from(2.71828);

    println!("a = {}", a);
    println!("b = {}", b);
    println!("a + b = {}", a + b);
    println!("a * b = {}", a * b);
    println!("a / b = {}", a / b);
    println!("a.sqrt() = {}", a.sqrt());
    println!("a.sin() = {}", a.sin());
    println!("a.cos() = {}", a.cos());

    let vec1 = FixedVec2::new(FixedPoint::from(3.0), FixedPoint::from(4.0));
    let vec2 = FixedVec2::new(FixedPoint::from(1.0), FixedPoint::from(2.0));

    println!("vec1 = {}", vec1);
    println!("vec2 = {}", vec2);
    println!("vec1 + vec2 = {}", vec1 + vec2);
    println!("vec1.dot(vec2) = {}", vec1.dot(vec2));
    println!("vec1.length() = {}", vec1.length());
    println!("vec1.normalize() = {}", vec1.normalize());

    let angle = FixedPoint::from(std::f32::consts::PI / 4.0);
    let rotated = vec1.rotate(angle);
    println!("vec1 rotated by 45° = {}", rotated);

    println!("PI = {}", FixedPoint::PI);
    println!("E = {}", FixedPoint::E);
}
