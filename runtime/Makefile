CC ?= cc
AR ?= ar
CFLAGS ?= -std=c11 -O2 -fPIC -I.

RUNTIME_OBJECT := runtime.aura.o
RUNTIME_ARCHIVE := libaurart.a
LLVM_RUNTIME_OBJECT := runtime.llvm.aura.o
LLVM_RUNTIME_ARCHIVE := libaurart-llvm.a

.PHONY: all llvm clean

all: $(RUNTIME_ARCHIVE)

llvm: $(LLVM_RUNTIME_ARCHIVE)

$(RUNTIME_OBJECT): runtime.c
	$(CC) $(CFLAGS) -c $< -o $@

$(RUNTIME_ARCHIVE): $(RUNTIME_OBJECT)
	$(AR) rcs $@ $<

$(LLVM_RUNTIME_OBJECT): runtime.c
	$(CC) $(CFLAGS) -Wno-implicit-function-declaration -DAURA_LLVM_RUNTIME -DAURA_RUNTIME_NO_MAIN -c $< -o $@

$(LLVM_RUNTIME_ARCHIVE): $(LLVM_RUNTIME_OBJECT)
	$(AR) rcs $@ $<

clean:
	rm -f $(RUNTIME_OBJECT) $(RUNTIME_ARCHIVE) $(LLVM_RUNTIME_OBJECT) $(LLVM_RUNTIME_ARCHIVE)
