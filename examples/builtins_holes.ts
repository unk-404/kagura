let values = [1, , 3];
console.log(values.map(value => value * 2).join(","));
console.log(values.filter(value => true).join(","));
let count = 0;
values.forEach(value => {
  count = count + 1;
});
console.log(count);
console.log(values.some(value => value === undefined));
console.log(values.indexOf(undefined));
console.log(values.includes(undefined));
console.log(values.reduce((a, b) => a + b, 0));
