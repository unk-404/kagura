var values = [];
Array.prototype.push.call(values, 1, 2);
console.log(values.join(","), values.length);
console.log(Array.prototype.isPrototypeOf(values));
console.log(Object.prototype.toString.call(values));
console.log([1, 2].toString());
