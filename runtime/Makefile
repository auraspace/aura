CC ?= cc
AR ?= ar
CFLAGS ?= -std=c11 -O2 -fPIC -I.
RUNTIME_CFLAGS ?= $(CFLAGS)
LLVM_RUNTIME_CFLAGS ?= $(CFLAGS) -Wno-implicit-function-declaration -DAURA_LLVM_RUNTIME -DAURA_RUNTIME_NO_MAIN

RUNTIME_OBJECT := runtime.aura.o
RUNTIME_ARCHIVE := libaurart.a
LLVM_RUNTIME_OBJECT := runtime.llvm.aura.o
LLVM_RUNTIME_ARCHIVE := libaurart-llvm.a

.PHONY: all llvm clean

all: $(RUNTIME_ARCHIVE)

llvm: $(LLVM_RUNTIME_ARCHIVE)

$(RUNTIME_OBJECT): runtime.c
	@mkdir -p "$(@D)"
	$(CC) $(RUNTIME_CFLAGS) -c $< -o $@

$(RUNTIME_ARCHIVE): $(RUNTIME_OBJECT)
	@mkdir -p "$(@D)"
	$(AR) rcs $@ $<

$(LLVM_RUNTIME_OBJECT): runtime.c
	@mkdir -p "$(@D)"
	$(CC) $(LLVM_RUNTIME_CFLAGS) -c $< -o $@

$(LLVM_RUNTIME_ARCHIVE): $(LLVM_RUNTIME_OBJECT)
	@mkdir -p "$(@D)"
	$(AR) rcs $@ $<

clean:
	rm -f $(RUNTIME_OBJECT) $(RUNTIME_ARCHIVE) $(LLVM_RUNTIME_OBJECT) $(LLVM_RUNTIME_ARCHIVE)
