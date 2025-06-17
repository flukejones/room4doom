use math::FixedPoint;

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

    println!("PI = {}", FixedPoint::PI);
    println!("E = {}", FixedPoint::E);
}
