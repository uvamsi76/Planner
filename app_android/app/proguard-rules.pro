# kotlinx.serialization ships its own R8 rules; keep our @Serializable model
# classes' generated serializers reachable.
-keepclassmembers @kotlinx.serialization.Serializable class dev.vamsi.planner.** {
    *** Companion;
    kotlinx.serialization.KSerializer serializer(...);
}
