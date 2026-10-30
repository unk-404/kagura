let error = new Error("boom");
console.log(error.name, error.message);
let type_error = new TypeError("bad");
console.log(type_error.name, type_error.message);
try {
  throw new TypeError("thrown");
} catch (caught) {
  console.log(caught.name, caught.message);
}
