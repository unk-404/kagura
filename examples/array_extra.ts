console.log([1, 2, 3].copyWithin(0, 1).join(","));
console.log([1, [2, [3]]].flat(2).join(","));
console.log([1, 2].flatMap((value) => [value, value]).join(","));
console.log([1, 2, 3].reduceRight((left, right) => left - right));
