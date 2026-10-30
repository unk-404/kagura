var value = JSON.parse('{"a":1,"b":[true,null,"x"],"c":{"d":2}}');
console.log(value.a, value.b[1], value.b[2], value.c.d);
console.log(JSON.stringify(value));
console.log(JSON.parse('[1,2,3]').length);
console.log(JSON.parse('"hello"'));
