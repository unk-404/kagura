let object = { value: 5 };
object.get = function () {
  return this.value;
};
console.log(object.get());
let detached = object.get;
try {
  detached();
} catch (error) {
  console.log("no receiver");
}
