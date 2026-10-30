let object = { value: 7 };
object.get = function () {
  let inner = () => this.value;
  return inner();
};
object.make = function () {
  return () => this.value;
};
console.log(object.get());
let get = object.make();
console.log(get());
