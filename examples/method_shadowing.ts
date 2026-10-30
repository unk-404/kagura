let object = {};
object.map = function (value) {
  return value + 10;
};
console.log(object.map(1));
let array = [1, 2];
console.log(array.map(value => value * 2).join(","));
