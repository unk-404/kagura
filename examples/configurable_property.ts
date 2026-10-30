let object = {};
Object.defineProperty(object, "x", { value: 1, configurable: false });
try {
  Object.defineProperty(object, "x", { value: 2 });
} catch (error) {
  console.log("locked");
}
console.log(object.x);
Object.defineProperty(object, "x", { value: 1 });
console.log("same");
