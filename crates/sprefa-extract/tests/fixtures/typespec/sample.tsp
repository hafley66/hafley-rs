import "@typespec/http";

using TypeSpec.Http;

namespace Pets;

scalar PetId extends string;

enum Kind {
  dog,
  cat: "cat",
}

model Pet {
  id: PetId;
  name: string;
  kind?: Kind;
  tags: string[];
}

model Owner {
  @key id: int64;
  pets: Pet[];
}

@route("/pets")
op listPets(@query limit?: int32): Pet[];
