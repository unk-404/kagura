let base = { inherited: 1 };
let object = Object.create(base);
object.own = 2;
console.log(object.hasOwnProperty("inherited"));
console.log(object.hasOwnProperty("own"));
console.log(Object.is(0 / 0, 0 / 0));
console.log(Object.is(0, -0));
console.log(Array.isArray([]));
console.log(Array.isArray({}));
