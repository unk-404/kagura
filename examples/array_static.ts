console.log(Array.of(1, 2, 3).join(","));
let arrayLike = { 0: "a", 1: "b", length: 2 };
console.log(Array.from(arrayLike).join(","));
console.log(Array.from([1, 2]).join(","));
console.log(Object.hasOwn({ x: 1 }, "x"));
console.log(Object.hasOwn({ x: 1 }, "y"));
