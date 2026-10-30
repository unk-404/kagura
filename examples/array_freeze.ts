let array = [1, 2];
Object.freeze(array);
array[0] = 3;
array.push(4);
array.length = 0;
console.log(array.join(","));
console.log(array.length);
console.log(Object.isFrozen(array));
