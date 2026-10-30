let values = [1, 2, 3, 4];
console.log(values.some(value => value > 3));
console.log(values.every(value => value > 0));
console.log(values.find(value => value % 2 === 0));
console.log(values.reduce((a, b) => a + b, 0));
console.log(values.reduce((a, b) => a + b));
