let letters = ["b", "a", "c"];
console.log(letters.sort().join(","));
let numbers = [3, 1, 10, 2];
console.log(numbers.sort((a, b) => a - b).join(","));
console.log([10, 2, 1].sort().join(","));
let holes = [3, , 1];
console.log(holes.sort().join(","));
