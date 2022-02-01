from pyorchard import *

builder = Builder.default()
output = Output.default()
builder.add_output(output)
rng = Random.default()
sighash = 32*b"\x00"
bundle = builder.build(rng)
pk = ProvingKey.build()
bundle.prepare(rng, sighash)
bundle.create_proof(pk, rng)
bundle.finalize()
serialized = bundle.serialized()
print(serialized)