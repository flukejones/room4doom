There are a lot of cyclic dependencies in Doom.

Thinkers maintain pointers to each other so that they can be iterated over in `O(n)` time
where `n` is the number of Thinkers actually active. The linked list was required in Doom
due to the way the allocations work.

Thinkers that are map-objects generally maintain a pointer to a subsector they reside in.
The subsector also maintains a pointer to a Thinker, and each Thinker maintains a pointer
to others forming yet another linked list.