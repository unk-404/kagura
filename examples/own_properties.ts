let object = {};
Object.defineProperty(object, "hidden", { value: 1, enumerable: false });
console.log(Object.getOwnPropertyNames(object).join(","));
Object.defineProperties(object, {
  a: { value: 2, enumerable: true },
  b: { value: 3, enumerable: false },
});
console.log(object.a);
console.log(Object.keys(object).join(","));
let child = Object.create({ p: 1 }, { q: { value: 2, enumerable: true } });
console.log(child.p);
console.log(child.q);
