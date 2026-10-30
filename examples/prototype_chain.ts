let base = { x: 1 };
let object = Object.create(base);
object.y = 2;
console.log(object.x);
console.log(object.y);
console.log("x" in object);
console.log(Object.getPrototypeOf(object) === base);
Object.setPrototypeOf(object, { x: 3 });
console.log(object.x);
