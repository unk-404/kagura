function join(separator, ...values) {
  return values.join(separator);
}
console.log(join("-", 1, 2, 3));
console.log(join("-"));
let count = (...values) => values.length;
console.log(count(1, 2));
