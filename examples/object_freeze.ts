let object = { value: 1 };
Object.freeze(object);
object.value = 2;
object.extra = 3;
console.log(object.value);
console.log(object.extra);
console.log(Object.isFrozen(object));
let normal = { a: 1 };
console.log(Object.isFrozen(normal));
