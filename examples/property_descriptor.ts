let object = {};
Object.defineProperty(object, "hidden", {
  value: 42,
  enumerable: false,
  writable: false,
});
console.log(object.hidden);
let descriptor = Object.getOwnPropertyDescriptor(object, "hidden");
console.log(descriptor.value);
console.log(descriptor.enumerable);
console.log(descriptor.writable);
console.log(Object.keys(object).length);
