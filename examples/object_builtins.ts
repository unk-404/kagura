let object = { a: 1, b: 2 };
console.log(Object.keys(object).join(","));
console.log(Object.values(object).join(","));
let entries = Object.entries(object);
console.log(entries[0].join(":"));
let array = [1, , 3];
console.log(Object.keys(array).join(","));
